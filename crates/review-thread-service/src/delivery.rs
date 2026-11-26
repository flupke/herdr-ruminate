use std::collections::{HashMap, HashSet};
use std::sync::{
    Arc, Condvar, Mutex, PoisonError,
    mpsc::{self, Receiver, Sender},
};
use std::thread;
use std::time::Duration;

use agent_hooks::{AgentHooks, Heard, OtherPrompts};
use herdr_client::client::EventCanceller;
use herdr_client::protocol::{Agent, AgentPort, PaneId, PromptWait};

use crate::{Input, PinnedAgent};

const HOOK_LIMIT: Duration = Duration::from_secs(5);

const BLOCKED: &str = "The progressive reviewer stopped this prompt while it typed one of its \
                       own here, since text left in the input box joins its prompt. Empty the \
                       input box, keeping your own text elsewhere, before you Retry in the \
                       reviewer.";

fn prompt_agent(
    port: &dyn AgentPort,
    hooks: Option<&AgentHooks>,
    agent: &Agent,
    text: &str,
    waits: &mut dyn FnMut(Option<PromptWait>),
    withdrawal: &EventCanceller,
) -> Result<(), PromptError> {
    let failure = |error: herdr_client::Error| match error {
        herdr_client::Error::AgentNotStarted { .. } => PromptError::NotStarted,
        herdr_client::Error::Cancelled => PromptError::Cancelled,
        herdr_client::Error::NotTyped { reason } => PromptError::Delivery(format!(
            "Nothing reached the agent ({reason}). Look at the agent's pane, then Retry."
        )),
        error => PromptError::Unknown(error.to_string()),
    };
    let Some((hooks, hooked)) = hooks.and_then(|hooks| Some((hooks, hooks.hooked(port, agent)?)))
    else {
        return port
            .prompt_agent_telling_waits(&agent.pane_id, text, waits, withdrawal)
            .map_err(failure);
    };
    let others = OtherPrompts::for_host(port);
    let expectation = hooks.expect_prompt(&hooked, text, BLOCKED, others);
    port.submit_prompt_telling_waits(&agent.pane_id, text, waits, withdrawal)
        .map_err(failure)?;
    match expectation.wait(HOOK_LIMIT) {
        Some(Heard::Submitted) => Ok(()),
        Some(Heard::Joined { left }) => Err(PromptError::Blocked { left }),
        Some(Heard::Stopped) => Err(PromptError::Delivery(
            "The reviewer stopped its prompt, which reached the agent while it switched \
             sessions, or reached another session than Herdr reports. Look at the agent's pane, \
             then Retry."
                .into(),
        )),
        Some(Heard::Resumed | Heard::Blocked { .. }) | None => match port.get_agent(&agent.pane_id)
        {
            Ok(Some(now)) if now.agent_status.started_on_prompt() => Ok(()),
            Ok(_) => Err(PromptError::Unknown(format!(
                "the agent's hooks did not report the prompt within {} s",
                HOOK_LIMIT.as_secs()
            ))),
            Err(error) => Err(failure(error)),
        },
    }
}

type Outcome = Box<dyn FnOnce(Result<(), PromptError>) + Send>;

type Waits = Box<dyn FnMut(Option<PromptWait>) + Send>;

struct QueuedPrompt {
    agent: Agent,
    text: String,
    withdrawal: EventCanceller,
    waits: Waits,
    outcome: Outcome,
}

impl QueuedPrompt {
    fn send(mut self, port: &dyn AgentPort, hooks: Option<&AgentHooks>) {
        let result = if self.withdrawal.is_cancelled() {
            Err(PromptError::Cancelled)
        } else {
            prompt_agent(
                port,
                hooks,
                &self.agent,
                &self.text,
                &mut self.waits,
                &self.withdrawal,
            )
        };
        (self.outcome)(result);
    }
}

enum Parcel {
    Prompt(Box<QueuedPrompt>),
    #[cfg(test)]
    Flush(Sender<()>),
}

#[derive(Default)]
pub(super) struct PromptGate {
    state: Mutex<GateState>,
    changed: Condvar,
}

#[derive(Default)]
struct GateState {
    holds: usize,
    sending: HashSet<PaneId>,
    queued: usize,
    #[cfg(test)]
    held: bool,
}

impl PromptGate {
    fn lock(&self) -> std::sync::MutexGuard<'_, GateState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn send(&self, delivery: QueuedPrompt, port: &dyn AgentPort, hooks: Option<&AgentHooks>) {
        let mut state = self.lock();
        #[cfg(test)]
        if state.holds > 0 {
            state.held = true;
            self.changed.notify_all();
        }
        state = self
            .changed
            .wait_while(state, |state| state.holds > 0)
            .unwrap_or_else(PoisonError::into_inner);
        #[cfg(test)]
        {
            state.held = false;
        }
        let pane = delivery.agent.pane_id.clone();
        state.sending.insert(pane.clone());
        drop(state);
        let _sent = Sent { gate: self, pane };
        delivery.send(port, hooks);
    }

    fn queue(&self) {
        self.lock().queued += 1;
    }

    fn unqueue(&self) {
        self.lock().queued -= 1;
        self.changed.notify_all();
    }

    #[cfg(any(test, feature = "flush"))]
    fn settle(&self) {
        drop(
            self.changed
                .wait_while(self.lock(), |state| {
                    !state.sending.is_empty() || (state.queued > 0 && state.holds == 0)
                })
                .unwrap_or_else(PoisonError::into_inner),
        );
    }

    #[cfg(test)]
    fn wait_until_held(&self, guard: std::time::Duration) {
        let (state, waited) = self
            .changed
            .wait_timeout_while(self.lock(), guard, |state| !state.held)
            .unwrap_or_else(PoisonError::into_inner);
        drop(state);
        assert!(
            !waited.timed_out(),
            "the courier sent a prompt while a hold lived, or never got one"
        );
    }
}

struct Sent<'a> {
    gate: &'a PromptGate,
    pane: PaneId,
}

impl Drop for Sent<'_> {
    fn drop(&mut self) {
        let mut state = self.gate.lock();
        state.sending.remove(&self.pane);
        state.queued -= 1;
        drop(state);
        self.gate.changed.notify_all();
    }
}

#[must_use = "prompts are held only while the hold lives"]
pub struct PromptHold(Arc<PromptGate>);

impl PromptHold {
    fn new(gate: &Arc<PromptGate>) -> Self {
        gate.lock().holds += 1;
        gate.changed.notify_all();
        Self(Arc::clone(gate))
    }

    pub fn drained(&self, pane: &PaneId) -> Drained {
        Drained {
            gate: Arc::clone(&self.0),
            pane: pane.clone(),
        }
    }
}

pub struct Drained {
    gate: Arc<PromptGate>,
    pane: PaneId,
}

impl Drained {
    pub fn wait(self) {
        drop(
            self.gate
                .changed
                .wait_while(self.gate.lock(), |state| state.sending.contains(&self.pane))
                .unwrap_or_else(PoisonError::into_inner),
        );
    }
}

impl Drop for PromptHold {
    fn drop(&mut self) {
        self.0.lock().holds -= 1;
        self.0.changed.notify_all();
    }
}

impl std::fmt::Debug for PromptHold {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("PromptHold")
    }
}

pub(super) struct Courier {
    lanes: Mutex<HashMap<PaneId, Sender<Parcel>>>,
    port: Arc<dyn AgentPort>,
    gate: Arc<PromptGate>,
    hooks: Option<Arc<AgentHooks>>,
}

impl Courier {
    pub(super) fn start(
        port: Arc<dyn AgentPort>,
        gate: Arc<PromptGate>,
        hooks: Option<Arc<AgentHooks>>,
    ) -> Self {
        Self {
            lanes: Mutex::default(),
            port,
            gate,
            hooks,
        }
    }

    pub(super) fn hooks(&self) -> Option<&AgentHooks> {
        self.hooks.as_deref()
    }

    fn lane(&self, pane: &PaneId) -> Sender<Parcel> {
        let mut lanes = self.lanes.lock().unwrap_or_else(PoisonError::into_inner);
        lanes
            .entry(pane.clone())
            .or_insert_with(|| {
                let (sender, parcels) = mpsc::channel();
                let (port, gate, hooks) = (
                    Arc::clone(&self.port),
                    Arc::clone(&self.gate),
                    self.hooks.clone(),
                );
                thread::spawn(move || {
                    for parcel in parcels {
                        match parcel {
                            Parcel::Prompt(delivery) => {
                                gate.send(*delivery, &*port, hooks.as_deref());
                            }
                            #[cfg(test)]
                            Parcel::Flush(done) => {
                                let _ = done.send(());
                            }
                        }
                    }
                });
                sender
            })
            .clone()
    }

    pub(super) fn send(
        &self,
        agent: Agent,
        text: String,
        withdrawal: Option<EventCanceller>,
        outcome: impl FnOnce(Result<(), PromptError>) + Send + 'static,
    ) {
        self.send_telling_waits(agent, text, withdrawal, |_| {}, outcome);
    }

    pub(super) fn send_telling_waits(
        &self,
        agent: Agent,
        text: String,
        withdrawal: Option<EventCanceller>,
        waits: impl FnMut(Option<PromptWait>) + Send + 'static,
        outcome: impl FnOnce(Result<(), PromptError>) + Send + 'static,
    ) {
        let lane = self.lane(&agent.pane_id);
        let parcel = Parcel::Prompt(Box::new(QueuedPrompt {
            agent,
            text,
            withdrawal: withdrawal.unwrap_or_default(),
            waits: Box::new(waits),
            outcome: Box::new(outcome),
        }));
        self.gate.queue();
        if let Err(mpsc::SendError(Parcel::Prompt(delivery))) = lane.send(parcel) {
            (delivery.outcome)(Err(PromptError::Delivery(
                "The reviewer prompt courier stopped; nothing was sent".into(),
            )));
            self.gate.unqueue();
        }
    }

    #[cfg(test)]
    pub(super) fn flush(&self) {
        let lanes: Vec<Sender<Parcel>> = self
            .lanes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
            .cloned()
            .collect();
        for lane in lanes {
            let (done, flushed) = mpsc::channel();
            lane.send(Parcel::Flush(done)).unwrap();
            flushed.recv().unwrap();
        }
    }
}

#[derive(Clone)]
pub struct PromptSender {
    pub(super) sender: Sender<Input>,
    pub(super) gate: Arc<PromptGate>,
}

impl std::fmt::Debug for PromptSender {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PromptSender")
            .finish_non_exhaustive()
    }
}

pub struct PromptReceipt(Receiver<Result<(), PromptError>>);

#[derive(Debug, thiserror::Error)]
pub enum PromptError {
    #[error("The reviewer prompt was cancelled")]
    Cancelled,
    #[error("{0}")]
    Delivery(String),
    #[error("The agent did not start on the prompt. Look at the agent's pane, then Retry.")]
    NotStarted,
    #[error(
        "Text left in the agent's input box joined the prompt, so the reviewer stopped both. \
         Empty the agent's input box, then Retry. The text left there: {left}"
    )]
    Blocked { left: String },
    #[error("Delivery outcome unknown: {0}")]
    Unknown(String),
}

pub trait DispatchObserver: Send + Sync {
    fn before_attempt(&self, agent: &herdr_client::protocol::Agent) -> Result<(), String>;
    fn finished(&self, result: &Result<(), PromptError>) -> Result<(), String>;
}

impl PromptReceipt {
    pub fn wait(self) -> Result<(), PromptError> {
        self.0.recv().unwrap_or_else(|_| {
            Err(PromptError::Unknown(
                "The reviewer prompt dispatcher closed".into(),
            ))
        })
    }
}

#[derive(Debug)]
pub struct PromptCancellation {
    withdrawal: EventCanceller,
    worker: Sender<Input>,
}

impl Drop for PromptCancellation {
    fn drop(&mut self) {
        self.withdrawal.cancel();
        let _ = self.worker.send(Input::Withdrawn);
    }
}

impl PromptSender {
    pub fn hold(&self) -> PromptHold {
        PromptHold::new(&self.gate)
    }

    pub fn send(&self, agent: PinnedAgent, text: String) -> (PromptReceipt, PromptCancellation) {
        self.send_observed(agent, text, None)
    }

    pub fn publish_words(&self, words: review_words_bus::UserWords) {
        let _ = self
            .sender
            .send(Input::Bus(crate::BusInput::Publish(words)));
    }

    #[cfg(any(test, feature = "flush"))]
    pub fn flush(&self) {
        let (done, taken) = mpsc::channel();
        if self.sender.send(Input::Flush(done)).is_ok() {
            let _ = taken.recv();
        }
        self.gate.settle();
    }

    pub fn send_observed(
        &self,
        agent: PinnedAgent,
        text: String,
        observer: Option<Arc<dyn DispatchObserver>>,
    ) -> (PromptReceipt, PromptCancellation) {
        let withdrawal = EventCanceller::default();
        let (response, received) = mpsc::channel();
        if let Err(mpsc::SendError(Input::Prompt(request))) =
            self.sender.send(Input::Prompt(PromptRequest {
                agent,
                text,
                withdrawal: withdrawal.clone(),
                response,
                observer,
            }))
        {
            request.finish(Err(PromptError::Delivery(
                "The reviewer prompt dispatcher is unavailable; nothing was sent".into(),
            )));
        }
        (
            PromptReceipt(received),
            PromptCancellation {
                withdrawal,
                worker: self.sender.clone(),
            },
        )
    }
}

pub(super) struct PromptRequest {
    agent: PinnedAgent,
    text: String,
    withdrawal: EventCanceller,
    response: Sender<Result<(), PromptError>>,
    observer: Option<Arc<dyn DispatchObserver>>,
}

impl PromptRequest {
    fn finish(&self, mut result: Result<(), PromptError>) {
        if let Some(observer) = &self.observer
            && let Err(error) = observer.finished(&result)
        {
            result = Err(PromptError::Unknown(error));
        }
        let _ = self.response.send(result);
    }

    fn begin(&self, port: &dyn AgentPort) -> Result<Option<Agent>, PromptError> {
        if self.withdrawal.is_cancelled() {
            return Err(PromptError::Cancelled);
        }
        let Some(agent) = self.agent.current(port).map_err(PromptError::Delivery)? else {
            return Ok(None);
        };
        if self.withdrawal.is_cancelled() {
            return Err(PromptError::Cancelled);
        }
        if let Some(observer) = &self.observer {
            observer
                .before_attempt(&agent)
                .map_err(PromptError::Delivery)?;
        }
        self.agent.seal_attempt().map_err(PromptError::Delivery)?;
        Ok(Some(agent))
    }
}

#[derive(Default)]
pub(super) struct PromptQueue(Vec<PromptRequest>);

impl PromptQueue {
    pub(super) fn push(&mut self, request: PromptRequest) {
        self.0.push(request);
    }

    pub(super) fn is_pending(&self) -> bool {
        !self.0.is_empty()
    }

    pub(super) fn poll(&mut self, port: &dyn AgentPort, courier: &Courier) {
        for request in std::mem::take(&mut self.0) {
            match request.begin(port) {
                Ok(Some(agent)) => {
                    let text = request.text.clone();
                    let withdrawal = request.withdrawal.clone();
                    courier.send(agent, text, Some(withdrawal), move |result| {
                        request.finish(result);
                    });
                }
                Ok(None) => self.0.push(request),
                Err(error) => request.finish(Err(error)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;
    use herdr_client::memory::InMemoryAgents;

    const GUARD: Duration = Duration::from_secs(10);

    fn agent_in(pane: &str) -> Agent {
        serde_json::from_value(serde_json::json!({"pane_id": pane, "tab_id": "tab", "workspace_id": "workspace", "agent_status": "idle"})).unwrap()
    }

    fn worker(agents: &InMemoryAgents, directory: &std::path::Path) -> crate::Worker {
        let store = review_store::ReviewStore::open(directory.join("state"), directory).unwrap();
        crate::Worker::start(
            store,
            agents.clone(),
            None,
            herdr_client::protocol::AgentTarget::new(
                herdr_client::protocol::TabId("tab".into()),
                None,
            ),
            Err("No MCP listener in this test".into()),
            |_| Err("No MCP listener in this test".into()),
            |_| {},
        )
    }

    fn typed(agents: &InMemoryAgents) -> Vec<(String, String)> {
        agents
            .prompts()
            .into_iter()
            .map(|prompt| (prompt.pane_id.0, prompt.text))
            .collect()
    }

    fn outcome(receipt: &PromptReceipt) -> Result<(), PromptError> {
        receipt
            .0
            .recv_timeout(GUARD)
            .expect("the prompt never got its outcome")
    }

    #[test]
    fn a_prompt_waiting_on_one_agents_line_holds_none_to_another_and_those_to_it_keep_their_order()
    {
        let directory = tempfile::tempdir().unwrap();
        let agents = InMemoryAgents::default();
        let (a, b) = (agent_in("a"), agent_in("b"));
        agents.upsert_agent(a.clone());
        agents.upsert_agent(b.clone());
        agents.type_in_line(&a.pane_id);
        let worker = worker(&agents, directory.path());
        let prompts = worker.prompt_sender();

        let (first, _first) = prompts.send(PinnedAgent::new(a.clone()), "first to A".into());
        agents.wait_until_waiting(&a.pane_id, 1);
        let (second, _second) = prompts.send(PinnedAgent::new(a.clone()), "second to A".into());
        let (to_b, _to_b) = prompts.send(PinnedAgent::new(b), "to B".into());

        outcome(&to_b).unwrap();
        assert_eq!(typed(&agents), [("b".into(), "to B".into())]);
        agents.empty_line(&a.pane_id);
        outcome(&first).unwrap();
        outcome(&second).unwrap();
        assert_eq!(
            typed(&agents),
            [
                ("b".into(), "to B".into()),
                ("a".into(), "first to A".into()),
                ("a".into(), "second to A".into()),
            ]
        );
    }

    #[test]
    fn a_prompt_withdrawn_while_it_waits_on_the_agents_line_is_not_typed_once_it_empties() {
        let directory = tempfile::tempdir().unwrap();
        let agents = InMemoryAgents::default();
        let a = agent_in("a");
        agents.upsert_agent(a.clone());
        agents.type_in_line(&a.pane_id);
        let worker = worker(&agents, directory.path());
        let prompts = worker.prompt_sender();

        let (receipt, cancellation) = prompts.send(PinnedAgent::new(a.clone()), "Review".into());
        agents.wait_until_waiting(&a.pane_id, 1);
        drop(cancellation);

        assert!(matches!(outcome(&receipt), Err(PromptError::Cancelled)));
        agents.empty_line(&a.pane_id);
        prompts.flush();
        assert_eq!(typed(&agents), []);
    }

    #[test]
    fn a_hold_drains_a_terminal_while_a_prompt_to_another_waits_on_its_line() {
        let directory = tempfile::tempdir().unwrap();
        let agents = InMemoryAgents::default();
        let (a, b) = (agent_in("a"), agent_in("b"));
        agents.upsert_agent(a.clone());
        agents.upsert_agent(b.clone());
        agents.type_in_line(&a.pane_id);
        let worker = worker(&agents, directory.path());
        let prompts = worker.prompt_sender();
        let (to_a, _to_a) = prompts.send(PinnedAgent::new(a.clone()), "to A".into());
        agents.wait_until_waiting(&a.pane_id, 1);

        let hold = prompts.hold();
        hold.drained(&b.pane_id).wait();
        let emptying = {
            let (agents, pane) = (agents.clone(), a.pane_id.clone());
            thread::spawn(move || agents.empty_line(&pane))
        };
        hold.drained(&a.pane_id).wait();

        assert_eq!(
            typed(&agents),
            [("a".to_owned(), "to A".to_owned())],
            "A's terminal drained before its prompt went"
        );
        emptying.join().unwrap();
        outcome(&to_a).unwrap();
        drop(hold);
    }

    struct OutcomeObserver(AtomicBool);

    impl DispatchObserver for OutcomeObserver {
        fn before_attempt(&self, _: &herdr_client::protocol::Agent) -> Result<(), String> {
            panic!("A closed queue must never start an external attempt");
        }

        fn finished(&self, result: &Result<(), PromptError>) -> Result<(), String> {
            assert!(matches!(result, Err(PromptError::Delivery(_))));
            self.0.store(true, Ordering::Release);
            Ok(())
        }
    }

    #[test]
    fn a_held_courier_sends_nothing_until_the_hold_ends_and_skips_a_prompt_withdrawn_meanwhile() {
        let agents = herdr_client::memory::InMemoryAgents::default();
        let agent: Agent = serde_json::from_value(serde_json::json!({"pane_id":"pane", "tab_id":"tab", "workspace_id":"workspace", "agent_status":"idle"})).unwrap();
        agents.upsert_agent(agent.clone());
        let gate = Arc::<PromptGate>::default();
        let courier = Courier::start(Arc::new(agents.clone()), gate.clone(), None);
        let hold = PromptHold::new(&gate);
        let (outcomes, outcome) = mpsc::channel();
        let withdrawn = EventCanceller::default();
        for (text, withdrawal) in [("first", None), ("withdrawn", Some(withdrawn.clone()))] {
            let outcomes = outcomes.clone();
            courier.send(agent.clone(), text.into(), withdrawal, move |result| {
                outcomes.send((text, result.is_ok())).unwrap();
            });
        }

        gate.wait_until_held(std::time::Duration::from_secs(10));
        assert!(agents.prompts().is_empty(), "nothing is sent while held");
        withdrawn.cancel();
        drop(hold);
        courier.flush();

        assert_eq!(
            agents
                .prompts()
                .iter()
                .map(|prompt| prompt.text.as_str())
                .collect::<Vec<_>>(),
            ["first"]
        );
        assert_eq!(outcome.recv().unwrap(), ("first", true));
        assert_eq!(outcome.recv().unwrap(), ("withdrawn", false));
    }

    struct SavedOutcome(AtomicBool);

    impl DispatchObserver for SavedOutcome {
        fn before_attempt(&self, _: &herdr_client::protocol::Agent) -> Result<(), String> {
            Ok(())
        }

        fn finished(&self, _: &Result<(), PromptError>) -> Result<(), String> {
            self.0.store(true, Ordering::Release);
            Ok(())
        }
    }

    #[test]
    fn a_flush_passes_a_held_prompt_and_returns_once_a_sent_one_has_its_outcome_saved() {
        let directory = tempfile::tempdir().unwrap();
        let store =
            review_store::ReviewStore::open(directory.path().join("state"), directory.path())
                .unwrap();
        let agents = herdr_client::memory::InMemoryAgents::default();
        let agent: Agent = serde_json::from_value(serde_json::json!({"pane_id":"pane", "tab_id":"tab", "workspace_id":"workspace", "agent_status":"idle"})).unwrap();
        agents.upsert_agent(agent.clone());
        let worker = crate::Worker::start(
            store,
            agents.clone(),
            None,
            herdr_client::protocol::AgentTarget::new(
                agent.tab_id.clone(),
                Some(agent.pane_id.clone()),
            ),
            Err("No MCP listener in this test".into()),
            |_| Err("No MCP listener in this test".into()),
            |_| {},
        );
        let prompts = worker.prompt_sender();
        let hold = prompts.hold();
        let saved = Arc::new(SavedOutcome(AtomicBool::new(false)));
        let (_receipt, _cancellation) =
            prompts.send_observed(PinnedAgent::new(agent), "Held".into(), Some(saved.clone()));

        prompts.flush();
        assert!(agents.prompts().is_empty(), "the held prompt is not sent");
        assert!(!saved.0.load(Ordering::Acquire));

        drop(hold);
        prompts.flush();
        assert_eq!(
            agents
                .prompts()
                .iter()
                .map(|prompt| prompt.text.as_str())
                .collect::<Vec<_>>(),
            ["Held"]
        );
        assert!(saved.0.load(Ordering::Acquire), "its outcome is saved");
    }

    #[test]
    fn a_prompt_the_host_refused_did_not_reach_the_agent_and_says_why() {
        let agent: Agent = serde_json::from_value(serde_json::json!({"pane_id":"pane", "tab_id":"tab", "workspace_id":"workspace", "agent_status":"blocked"})).unwrap();
        let port = herdr_client::memory::InMemoryAgents::default();
        port.refuse_prompts("it waits on its user");

        let result = prompt_agent(
            &port,
            None,
            &agent,
            "Review x.rs",
            &mut |_| {},
            &EventCanceller::default(),
        );

        assert!(
            matches!(&result, Err(PromptError::Delivery(message)) if message.contains("(it waits on its user)")),
            "{result:?}"
        );
    }

    #[test]
    fn closed_queue_reports_definite_non_delivery_before_returning_a_receipt() {
        let (sender, receiver) = mpsc::channel();
        drop(receiver);
        let agent = serde_json::from_value(serde_json::json!({"pane_id":"pane", "tab_id":"tab", "workspace_id":"workspace", "agent_status":"idle"})).unwrap();
        let observer = Arc::new(OutcomeObserver(AtomicBool::new(false)));
        let (receipt, _guard) = PromptSender {
            sender,
            gate: Arc::default(),
        }
        .send_observed(
            PinnedAgent::new(agent),
            "Prepared prompt".into(),
            Some(observer.clone()),
        );
        assert!(observer.0.load(Ordering::Acquire));
        assert!(matches!(receipt.wait(), Err(PromptError::Delivery(_))));
    }
}
