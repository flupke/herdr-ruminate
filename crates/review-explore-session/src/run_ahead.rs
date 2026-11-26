mod answer;
mod calls;
mod clean;
mod finishing;
mod settle;
mod switch;
mod take;
mod talk;

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use agent_fork::ProcessStamp;
use herdr_client::protocol::{Agent, AgentStatus, PaneId};
use review_explore::{InterviewUpdate, Question, TurnRequest};
use review_explore_runner::TurnInstructions;
use review_run_ahead::{
    DiscardReason, ForkEnd, ForkHost, ForkPoint, HostFields, OwnedFork, PaneWatch, SwitchFailure,
    TurnPath,
};
use review_types::ReviewUnit;

use crate::{ExploreSession, Input};

pub use talk::TALK_QUIET;

#[derive(Debug)]
pub struct RunAheadInput(Event);

#[derive(Debug)]
enum Event {
    Status {
        generation: u64,
        status: AgentStatus,
    },
    Ended {
        round: RoundKey,
        session: String,
        end: ForkEnd,
    },
    Cleaned {
        round: RoundKey,
        session: String,
    },
    Switched {
        round: RoundKey,
        request: String,
        result: Box<Result<Agent, SwitchFailure>>,
    },
    Settled {
        round: RoundKey,
        fork: String,
        result: Box<Result<Agent, SwitchFailure>>,
    },
    Quiet {
        number: u64,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RoundKey {
    unit: ReviewUnit,
    instance: String,
}

pub(crate) struct RunAheadState {
    host: Arc<dyn ForkHost>,
    reviewer: ProcessStamp,
    generation: u64,
    armed: Option<Armed>,
    discarded: HashSet<String>,
    finishing: Option<finishing::Finishing>,
    switching: Option<switch::Switching>,
    settling: Option<settle::Settling>,
    settle_failed: HashSet<String>,
    left_unrecorded: HashSet<String>,
    held: Option<crate::turn::SavedTurn>,
    paths: BTreeMap<String, TurnPath>,
    close_wait: std::time::Duration,
    talk_quiet: std::time::Duration,
    quiet_waits: u64,
    unavailable: Option<String>,
}

struct Armed {
    generation: u64,
    asked: Asked,
    asked_at_ms: u64,
    _watch: PaneWatch,
    taken: Option<Taken>,
    worked: bool,
    refusal: Option<String>,
    refusal_counted: bool,
    talk: Option<talk::Talk>,
    reserved: BTreeMap<String, ReservedIds>,
}

#[derive(Clone)]
struct Asked {
    round: RoundKey,
    question: Question,
    number: Option<usize>,
    pane: PaneId,
}

impl Asked {
    fn label(&self) -> String {
        label(self.number)
    }
}

fn label(number: Option<usize>) -> String {
    number.map_or_else(|| "Q?".to_owned(), |number| format!("Q{number}"))
}

#[derive(Clone, Debug)]
struct ReservedIds {
    request: String,
    answer: String,
}

impl ReservedIds {
    fn new() -> Self {
        Self {
            request: uuid::Uuid::new_v4().to_string(),
            answer: uuid::Uuid::new_v4().to_string(),
        }
    }

    fn apply(&self, request: &mut TurnRequest) {
        request.request.clone_from(&self.request);
        if let Some(answer) = &mut request.answer {
            answer.id.clone_from(&self.answer);
        }
    }
}

struct Taken {
    point: ForkPoint,
    at_ms: u64,
    forks: Vec<TakenFork>,
    files: take::ForkFiles,
}

#[derive(Clone)]
struct TakenFork {
    session: String,
    access: String,
    choice: String,
    request: TurnRequest,
    turn: TurnInstructions,
    host: HostFields,
    kept: Option<InterviewUpdate>,
    ended: bool,
}

impl RunAheadState {
    pub(crate) fn new(host: Arc<dyn ForkHost>, talk_quiet: std::time::Duration) -> Self {
        Self {
            host,
            reviewer: ProcessStamp::read(std::process::id()),
            generation: 0,
            armed: None,
            discarded: HashSet::new(),
            finishing: None,
            switching: None,
            settling: None,
            settle_failed: HashSet::new(),
            left_unrecorded: HashSet::new(),
            held: None,
            paths: BTreeMap::new(),
            close_wait: clean::CLOSE_WAIT,
            talk_quiet,
            quiet_waits: 0,
            unavailable: None,
        }
    }

    pub(crate) fn paths_again(&mut self, paths: BTreeMap<String, TurnPath>) {
        self.paths = paths;
    }

    pub(crate) fn turn_paths(&self) -> &BTreeMap<String, TurnPath> {
        &self.paths
    }

    // Codex's calls name their session.
    fn fork_of_call(&self, access: &str, session: Option<&str>) -> Option<(&TakenFork, &RoundKey)> {
        let waiting = self.armed.as_ref().into_iter().flat_map(|armed| {
            armed
                .taken
                .iter()
                .flat_map(|taken| &taken.forks)
                .map(|fork| (fork, &armed.asked.round))
        });
        let finishing = self
            .finishing
            .as_ref()
            .map(|finishing| (&finishing.fork, &finishing.asked.round));
        let switching = self
            .switching
            .as_ref()
            .map(|switching| (&switching.fork, &switching.asked.round));
        waiting
            .chain(finishing)
            .chain(switching)
            .find(|(fork, _)| fork.access == access || session == Some(fork.session.as_str()))
    }

    fn fork_mut(&mut self, session: &str) -> Option<&mut TakenFork> {
        let waiting = self
            .armed
            .as_mut()
            .and_then(|armed| armed.taken.as_mut())
            .into_iter()
            .flat_map(|taken| &mut taken.forks);
        waiting
            .chain(self.finishing.as_mut().map(|finishing| &mut finishing.fork))
            .find(|fork| fork.session == session)
    }

    fn log(&self, line: &str) {
        let asked = self
            .armed
            .as_ref()
            .map(|armed| &armed.asked)
            .or_else(|| self.finishing.as_ref().map(|finishing| &finishing.asked));
        let label = label(asked.and_then(|asked| asked.number));
        self.host.log(&format!("{label}: {line}"));
    }
}

impl TakenFork {
    fn trace(&self) -> review_run_ahead::ForkTrace<'_> {
        review_run_ahead::ForkTrace {
            session: &self.session,
            host: &self.host,
        }
    }
}

impl Armed {
    fn reserved_for(&self, request: &TurnRequest) -> Option<&ReservedIds> {
        if self.asked.round.instance != request.instance {
            return None;
        }
        let answer = request.answer.as_ref()?;
        let question = answer.question.as_ref()?;
        if !self
            .asked
            .question
            .is_version(&question.id, question.version)
        {
            return None;
        }
        self.reserved.get(&answer.option.as_ref()?.id)
    }

    fn is_for(&self, round: &RoundKey, question: &Question) -> bool {
        self.asked.round == *round
            && self
                .asked
                .question
                .is_version(&question.id, question.version)
    }
}

impl ExploreSession {
    pub(crate) fn run_ahead_reconcile(&mut self) {
        self.run_ahead_withdraw_finishing();
        let waiting = self.question_for_forks();
        let rounds = self.rounds.clone();
        let on = move || {
            rounds
                .run_ahead()
                .is_ok_and(review_explore_round_settings::RunAhead::is_on)
        };
        let armed_for_it = self.run_ahead.armed.as_ref().is_some_and(|armed| {
            waiting
                .as_ref()
                .is_some_and(|(round, question)| armed.is_for(round, question))
        });
        if armed_for_it {
            if !on() {
                self.run_ahead_discard(DiscardReason::TurnedOff);
            }
            return;
        }
        self.run_ahead_discard(DiscardReason::QuestionGone);
        if let Some((round, question)) = waiting
            && on()
        {
            self.run_ahead_arm(round, question);
        }
    }

    #[cfg(test)]
    pub(crate) fn moves_agent(&self) -> bool {
        self.run_ahead.switching.is_some() || self.run_ahead.settling.is_some()
    }

    #[cfg(test)]
    pub(crate) fn wait_on_close(&mut self, wait: std::time::Duration) {
        self.run_ahead.close_wait = wait;
    }

    fn run_ahead_available(&mut self, unavailable: Option<String>) {
        if self.run_ahead.unavailable != unavailable {
            self.run_ahead.unavailable.clone_from(&unavailable);
            let _ = self
                .events
                .send(ui_events::ExploreRunAheadUnavailable(unavailable));
        }
    }

    fn show_path(&mut self, round: &RoundKey, request: &str, path: TurnPath) {
        self.run_ahead
            .paths
            .insert(request.to_owned(), path.clone());
        let _ = self.events.send(ui_events::ExploreTurnPath {
            round: round.instance.clone(),
            request: request.to_owned(),
            path,
        });
    }

    fn question_for_forks(&self) -> Option<(RoundKey, Question)> {
        if self.state.storage_error.is_some() || self.state.historical {
            return None;
        }
        let round = self.state.round.as_ref()?;
        let question = round.exploration.waiting_turn()?.next.clone()?;
        let key = RoundKey {
            unit: round.exploration.comparison.checkpoint.review_unit.clone(),
            instance: round.exploration.instance.clone(),
        };
        Some((key, question))
    }

    fn update_forks<T>(
        &self,
        round: &RoundKey,
        change: impl FnOnce(&mut review_run_ahead::RoundForks) -> T,
    ) -> review_store::Result<T> {
        self.rounds
            .update_forks(&round.unit, &round.instance, change)
    }

    fn run_ahead_arm(&mut self, round: RoundKey, question: Question) {
        let Some(agent) = self.round_agent() else {
            return;
        };
        self.run_ahead.generation += 1;
        let generation = self.run_ahead.generation;
        let inbox = self.inbox.clone();
        let watch = self.run_ahead.host.watch(
            &agent.pane_id,
            Box::new(move |status| {
                inbox.deliver(Input::RunAhead(RunAheadInput(Event::Status {
                    generation,
                    status,
                })));
            }),
        );
        let saved = self.state.round.as_ref();
        let number = saved.and_then(|round| round.exploration.question_number(&question));
        let asked_at_ms = saved
            .and_then(|round| asked_at_ms(round, &question))
            .unwrap_or_else(review_explore::now_ms);
        let since_ms = saved
            .and_then(|round| asking_started_ms(round, &question))
            .unwrap_or(asked_at_ms);
        let talk = match self
            .rounds
            .unanswered_round_messages(&round.unit, &round.instance)
        {
            Ok(unanswered) => talk::Talk::waiting_for(
                unanswered
                    .into_iter()
                    .filter(|(_, posted)| posted.is_some_and(|posted| posted >= since_ms))
                    .map(|(message, _)| message)
                    .collect(),
            ),
            Err(error) => {
                self.run_ahead.host.log(&format!(
                    "{}: the round conversation is unknown, no talk holds the forks: {error}",
                    label(number)
                ));
                None
            }
        };
        self.run_ahead.armed = Some(Armed {
            generation,
            asked: Asked {
                round,
                question,
                number,
                pane: agent.pane_id.clone(),
            },
            asked_at_ms,
            _watch: watch,
            taken: None,
            worked: false,
            refusal: None,
            refusal_counted: false,
            talk,
            reserved: BTreeMap::new(),
        });
        self.run_ahead
            .log("the question waits; its forks start once the agent is idle");
        // Herdr reports status changes only: the agent may be idle already.
        if self
            .agents
            .get_agent(&agent.pane_id)
            .ok()
            .flatten()
            .is_some_and(|agent| agent.agent_status.waits_for_prompt())
        {
            self.run_ahead_idle();
        }
    }

    fn round_agent(&mut self) -> Option<Agent> {
        if let Some(agent) = self
            .state
            .agent
            .as_ref()
            .and_then(|agent| agent.current(&*self.agents).ok().flatten())
        {
            return Some(agent);
        }
        self.target.resolve(&*self.agents).ok().flatten()
    }

    pub(crate) fn run_ahead_input(&mut self, input: RunAheadInput) {
        match input.0 {
            Event::Status { generation, status } => self.run_ahead_status(generation, status),
            Event::Ended {
                round,
                session,
                end,
            } => self.run_ahead_ended(&round, &session, &end),
            Event::Cleaned { round, session } => self.run_ahead_cleaned(&round, &session),
            Event::Switched {
                round,
                request,
                result,
            } => self.run_ahead_switched(&round, &request, *result),
            Event::Settled {
                round,
                fork,
                result,
            } => self.run_ahead_settled(&round, &fork, *result),
            Event::Quiet { number } => self.run_ahead_quiet(number),
        }
    }

    fn run_ahead_status(&mut self, generation: u64, status: AgentStatus) {
        let Some(armed) = self
            .run_ahead
            .armed
            .as_mut()
            .filter(|armed| armed.generation == generation)
        else {
            return;
        };
        if matches!(status, AgentStatus::Working | AgentStatus::Blocked) {
            armed.worked |= armed.taken.is_some();
            if let Some(talk) = &mut armed.talk {
                talk.interrupt();
            }
            return;
        }
        if !status.waits_for_prompt() {
            return;
        }
        let Some(taken) = &armed.taken else {
            self.run_ahead_idle();
            return;
        };
        // Claude Code under herdr-core writes the end of its turn to its transcript after it
        // reports the turn ended.
        let worked = std::mem::take(&mut armed.worked);
        let moved = self.run_ahead.host.last_entry(&taken.point) != taken.point.entry
            || worked
                && self
                    .agents
                    .get_agent(&armed.asked.pane)
                    .ok()
                    .flatten()
                    .and_then(|agent| self.run_ahead.host.session(&agent))
                    .is_none_or(|session| session != taken.point.session);
        if moved {
            self.run_ahead
                .log("the agent's session moved since the forks were taken: taking them again");
            self.discard_taken(DiscardReason::SessionMoved);
            self.run_ahead_take();
        }
    }
}

enum Move {
    Resume { session: String, from: OwnedFork },
    Switch(OwnedFork),
}

impl Move {
    fn switch(trace: review_run_ahead::ForkTrace<'_>) -> Self {
        Self::Switch(OwnedFork::of(trace))
    }

    fn resume(session: String, from: review_run_ahead::ForkTrace<'_>) -> Self {
        Self::Resume {
            session,
            from: OwnedFork::of(from),
        }
    }

    fn start_once_drained(
        self,
        host: &Arc<dyn ForkHost>,
        hold: &review_thread_service::PromptHold,
        pane: &PaneId,
        done: Box<dyn FnOnce(Result<Agent, SwitchFailure>) + Send>,
    ) {
        let (host, drained, pane) = (Arc::clone(host), hold.drained(pane), pane.clone());
        std::thread::spawn(move || {
            drained.wait();
            match &self {
                Self::Resume { session, from } => host.resume(&pane, from.trace(), session, done),
                Self::Switch(fork) => host.switch(
                    review_run_ahead::SwitchTo {
                        pane: &pane,
                        fork: fork.trace(),
                    },
                    done,
                ),
            }
        });
    }
}

fn asking_request<'a>(
    round: &'a review_explore::ExploreRound,
    question: &Question,
) -> Option<&'a str> {
    let turn = round
        .exploration
        .conversation
        .iter()
        .rfind(|turn| turn.update.next.as_ref() == Some(question))?;
    Some(&turn.update.request)
}

fn asking_started_ms(round: &review_explore::ExploreRound, question: &Question) -> Option<u64> {
    round
        .turns
        .get(asking_request(round, question)?)?
        .started_at_ms
}

fn asked_at_ms(round: &review_explore::ExploreRound, question: &Question) -> Option<u64> {
    let started = asking_started_ms(round, question)?;
    let elapsed = round
        .exploration
        .agent_elapsed_ms
        .get(asking_request(round, question)?)?;
    Some(started.saturating_add(*elapsed))
}
