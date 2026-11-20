use std::collections::{BTreeMap, HashMap};
use std::ops::ControlFlow;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use herdr_client::protocol::{Agent, AgentPort, AgentTarget, HerdrEvent, PaneId, PromptWait};
use review_mcp::{Operation, Response};
use review_store::ReviewStore;
use review_threads::{
    Author, Message, Post, Recipient, Resolution, ReviewThreads, SavedDrafts, ThreadCommand,
    ThreadId, WakeupFailure, WakeupNotice, WakeupWait,
};
use review_types::ReviewUnit;
use review_words_bus::{Place, UserWords, WordsBus};
use serde::Serialize;
use vision_turns::TurnLog;

use crate::delivery::{Courier, PromptError, PromptQueue};
use crate::{
    BusInput, Command, Event, Input, NotificationNews, access::Access, notification::Notification,
    wakeup::Wakeup,
};

/// How often the worker checks the host again while a prompt or wakeup waits. A prompt bound to
/// a native session waits while the pane reports none, and Herdr publishes no event when an
/// agent reports its session again: only asking shows it. herdr-core tells it
/// ([`HerdrEvent::AgentSessionChanged`]), so such a prompt waits for that event there.
const RECHECK_INTERVAL: Duration = Duration::from_millis(100);

pub(super) struct State {
    store: ReviewStore,
    port: Arc<dyn AgentPort>,
    target: AgentTarget,
    available: bool,
    books: HashMap<ReviewUnit, ReviewThreads>,
    /// The threads of each review that the reviewer retried, or reopened, in this tab, with the
    /// latest comment each Retry covered: those come to this tab's agent whichever tab posted
    /// them.
    retried: HashMap<ReviewUnit, BTreeMap<ThreadId, u64>>,
    access: HashMap<String, Access>,
    notifications: HashMap<ReviewUnit, Notification>,
    wakeups: HashMap<String, Wakeup>,
    prompts: PromptQueue,
    courier: Courier,
    /// Where the outcomes of comment notifications come back, behind the other inputs.
    inputs: Sender<Input>,
    publish: Box<dyn Fn(Event) + Send>,
    /// A vision session's record of the wakeups sent, for its scripted agent.
    turns: Option<TurnLog>,
    /// Where the new review comments, and the words handed to the worker, are published for
    /// other programs; `None` publishes none.
    bus: Option<WordsBus>,
    /// The new review comments of each review that wait for the agent the next wakeup picks,
    /// whose pane they are published to.
    unpublished: HashMap<ReviewUnit, Vec<UserWords>>,
}

/// A comments wakeup as a vision session's turn log records it.
#[derive(Serialize)]
struct CommentsTurn<'a> {
    kind: &'static str,
    access: &'a str,
    text: &'a str,
}

impl State {
    pub(super) fn new(
        store: ReviewStore,
        port: Arc<dyn AgentPort>,
        target: AgentTarget,
        available: bool,
        publish: Box<dyn Fn(Event) + Send>,
        inputs: Sender<Input>,
        courier: Courier,
    ) -> Self {
        Self {
            store,
            courier,
            inputs,
            port,
            target,
            available,
            books: HashMap::new(),
            retried: HashMap::new(),
            access: HashMap::new(),
            notifications: HashMap::new(),
            wakeups: HashMap::new(),
            prompts: PromptQueue::default(),
            publish,
            turns: None,
            bus: None,
            unpublished: HashMap::new(),
        }
    }

    pub(super) fn run(mut self, receiver: &Receiver<Input>) {
        loop {
            let input = if self.needs_recheck() {
                receiver.recv_timeout(RECHECK_INTERVAL)
            } else {
                receiver.recv().map_err(|_| RecvTimeoutError::Disconnected)
            };
            match input {
                Ok(input) => {
                    if self.input(input).is_break() {
                        return;
                    }
                }
                Err(RecvTimeoutError::Disconnected) => return,
                Err(RecvTimeoutError::Timeout) => {}
            }
            self.poll();
        }
    }

    /// Handle one input; break once the owner stops the worker.
    fn input(&mut self, input: Input) -> ControlFlow<()> {
        match input {
            Input::Ui(command) => self.command(command),
            Input::Prompt(request) => self.prompts.push(request),
            #[cfg(any(test, feature = "flush"))]
            Input::Flush(done) => self.flush(&done),
            Input::Mcp(request) => {
                let result = self.request(
                    &request.access,
                    &request.operation,
                    request.agent_session.as_deref(),
                );
                request.respond(result);
            }
            Input::Notification {
                token,
                through,
                news,
            } => self.notification(&token, through, news),
            Input::LogTurns(turns) => self.turns = Some(turns),
            Input::Withdrawn => {}
            Input::Bus(input) => self.bus_input(input),
            Input::Stop => return ControlFlow::Break(()),
        }
        ControlFlow::Continue(())
    }

    /// Answers `done` once every earlier input is handled, and the words handed to the bus
    /// are published, or failed.
    #[cfg(any(test, feature = "flush"))]
    fn flush(&self, done: &std::sync::mpsc::Sender<()>) {
        if let Some(bus) = &self.bus {
            bus.flush();
        }
        let _ = done.send(());
    }

    fn bus_input(&mut self, input: BusInput) {
        match input {
            BusInput::Open(Err(error)) => (self.publish)(Event::Error(format!(
                "What you write to agents will not reach herdr-bus: {error}"
            ))),
            BusInput::Open(Ok(path)) => {
                let failures = self.inputs.clone();
                self.bus = Some(WordsBus::start(path, move |error| {
                    let _ = failures.send(Input::Bus(BusInput::Failed(error)));
                }));
            }
            BusInput::Publish(words) => {
                if let Some(bus) = &self.bus {
                    bus.publish(words);
                }
            }
            BusInput::Failed(error) => (self.publish)(Event::Error(error)),
        }
    }

    /// News of the notification sent for access `token`, for the comments `through` a sequence.
    fn notification(&mut self, token: &str, through: u64, news: NotificationNews) {
        match news {
            NotificationNews::Outcome(result) => self.notified(token, through, result),
            NotificationNews::Waits(wait) => self.notification_waits(token, wait),
        }
    }

    /// The notification sent for access `token`, for the comments `through` a sequence, has
    /// its outcome. One that failed is not sent again until the reviewer retries or posts
    /// another comment, which a wakeup requested since then already did.
    fn notified(&mut self, token: &str, through: u64, result: Result<(), PromptError>) {
        if let Some(wakeup) = self.wakeups.get_mut(token) {
            wakeup.hold(None);
        }
        let Err(error) = result else {
            return;
        };
        if self
            .wakeups
            .get(token)
            .is_some_and(|wakeup| wakeup.unchanged_since(through))
        {
            self.wakeups.remove(token);
        }
        let unit = self
            .access
            .get(token)
            .map(|access| access.review_unit.clone());
        self.report_wakeup(
            unit.as_ref(),
            Some(WakeupNotice::Failed(WakeupFailure {
                through,
                error: error.to_string(),
            })),
        );
    }

    /// The notification sent for access `token` waits for `wait` before the agent's host types it,
    /// or no longer waits: the threads its wakeup covers say so, with those of the notifications
    /// after it, which wait behind it.
    fn notification_waits(&mut self, token: &str, wait: Option<PromptWait>) {
        let Some(wakeup) = self.wakeups.get_mut(token) else {
            return;
        };
        wakeup.hold(wait);
        let notice = wakeup.held().map(|(wait, through)| {
            WakeupNotice::Waiting(WakeupWait {
                through,
                reason: wait.to_string(),
            })
        });
        let unit = self
            .access
            .get(token)
            .map(|access| access.review_unit.clone());
        // A wait alone is no error: without a review to name, nothing shows it.
        if let Some(unit) = &unit {
            self.report_wakeup(Some(unit), notice);
        }
    }

    /// The wakeup for the pending comments of `unit` cannot reach the agent: say why, and
    /// which comments wait.
    fn undelivered(&self, unit: Option<&ReviewUnit>, error: String) {
        match unit.and_then(|unit| {
            self.books
                .get(unit)?
                .pending_comment_sequence_for(&self.recipient(unit))
        }) {
            Some(through) => self.report_wakeup(
                unit,
                Some(WakeupNotice::Failed(WakeupFailure { through, error })),
            ),
            None => (self.publish)(Event::Error(error)),
        }
    }

    /// Publish what became of the latest wakeup for `unit`: `None` while one is on its way, why
    /// it waits to reach the agent, or why one did not reach it. Without a review or a waiting
    /// comment to name, a failure is only an error.
    fn report_wakeup(&self, unit: Option<&ReviewUnit>, notice: Option<WakeupNotice>) {
        match unit {
            Some(unit) => (self.publish)(Event::Wakeup {
                review_unit: unit.clone(),
                notice,
            }),
            None => {
                if let Some(WakeupNotice::Failed(failure)) = notice {
                    (self.publish)(Event::Error(failure.error));
                }
            }
        }
    }

    /// Whether something waits that only asking the host again may move: a prompt waiting for
    /// its agent's session, unless the host announces sessions; a notification; or a wakeup
    /// that needs a poll.
    fn needs_recheck(&self) -> bool {
        (self.prompts.is_pending() && !self.port.announces_sessions())
            || !self.notifications.is_empty()
            || self.wakeups.iter().any(|(token, wakeup)| {
                wakeup.needs_poll()
                    && self
                        .access
                        .get(token)
                        .is_some_and(|access| self.waits_for_agent(&access.review_unit))
            })
    }

    fn command(&mut self, command: Command) {
        match command {
            Command::Thread(command) => self.thread_command(command),
            Command::ActiveAgentChanged => self.refresh_target(),
            Command::Observe(event) => self.observe(event),
        }
    }

    fn thread_command(&mut self, command: ThreadCommand) {
        match command {
            ThreadCommand::Load(unit) => self.load_review(unit),
            ThreadCommand::SaveDraft { review_unit, draft } => {
                let result =
                    self.update_drafts(&review_unit, |drafts, threads| drafts.save(draft, threads));
                self.report(result);
            }
            ThreadCommand::DiscardDraft {
                review_unit,
                thread_id,
            } => {
                let result = self.update_drafts(&review_unit, |drafts, _| {
                    drafts.discard(&thread_id);
                    Ok(())
                });
                self.report(result);
            }
            ThreadCommand::Post { review_unit, post } => self.post_comment(&review_unit, post),
            ThreadCommand::SetResolution {
                review_unit,
                thread_id,
                resolution,
            } => {
                let result = self.set_resolution(&review_unit, &thread_id, resolution);
                self.report(result);
            }
            ThreadCommand::MarkRepliesRead {
                review_unit,
                messages,
            } => {
                let result = self.update(&review_unit, |book| {
                    book.mark_replies_read(&messages);
                    Ok(())
                });
                self.report(result);
            }
            ThreadCommand::MarkRead {
                review_unit,
                thread_id,
                through,
            } => {
                let result = self.update(&review_unit, |book| book.mark_read(&thread_id, through));
                self.report(result);
            }
            ThreadCommand::Retry {
                review_unit,
                thread_id,
            } => {
                let result = self
                    .load(&review_unit)
                    .and_then(|book| book.retry(&thread_id))
                    .and_then(|()| {
                        self.release(&review_unit, thread_id);
                        self.schedule(&review_unit, true)
                    });
                self.report(result);
            }
        }
    }

    fn load_review(&mut self, unit: ReviewUnit) {
        let mut result = self.load(&unit);
        let mut drafts = SavedDrafts::default();
        if result.is_ok() {
            drafts = self.drafts_to_restore(&unit);
            self.resume(&unit);
            result = Ok(self.books[&unit].clone());
        }
        (self.publish)(Event::Loaded(ui_events::ReviewThreadsLoaded {
            review_unit: unit,
            result,
            drafts,
        }));
    }

    /// The latest drafts saved for `unit`, as every reviewer process sees them.
    /// Unreadable drafts are reported but never hide the threads; they stay on disk.
    fn drafts_to_restore(&self, unit: &ReviewUnit) -> SavedDrafts {
        self.store.load_drafts(unit).unwrap_or_else(|error| {
            (self.publish)(Event::Error(format!("Could not load drafts: {error}")));
            SavedDrafts::default()
        })
    }

    fn set_resolution(
        &mut self,
        unit: &ReviewUnit,
        id: &review_threads::ThreadId,
        resolution: Resolution,
    ) -> Result<(), String> {
        self.update(unit, |book| book.set_resolution(id, resolution))?;
        if resolution == Resolution::Open {
            self.release(unit, id.clone());
            self.schedule(unit, true)?;
        } else if !self.waits_for_agent(unit) {
            self.cancel_wakeups(unit);
        }
        Ok(())
    }

    fn post_comment(&mut self, review_unit: &ReviewUnit, post: Post) {
        let post = post.from_tab(self.target.tab_id().0.clone());
        let message_id = post.message().id.clone();
        let thread_id = post.thread_id().clone();
        // What the bus gets of a review comment, an agent's reply aside.
        let words =
            (post.message().author == Author::Reviewer).then(|| post.message().text.clone());
        let result = self.update(review_unit, |book| {
            let new = book.message(&post.message().id).is_none();
            book.post(post)?;
            Ok(new)
        });
        let round = match &result {
            Ok(true) => self.books[review_unit]
                .thread(&thread_id)
                .and_then(|thread| thread.round())
                .map(str::to_owned),
            _ => None,
        };
        if let (Ok(true), Some(text)) = (&result, words) {
            self.unpublished
                .entry(review_unit.clone())
                .or_default()
                .push(UserWords {
                    id: message_id.as_str().to_owned(),
                    text,
                    review_unit: review_unit.clone(),
                    place: Place::Thread {
                        thread: thread_id.clone(),
                        round: round.clone(),
                    },
                    to: None,
                });
        }
        let result = result.map(|_| ());
        let posted = result.is_ok();
        if posted {
            // Updating the drafts forgets every draft whose post the threads now hold. A
            // crash before this write cannot restore the draft: loading forgets it too.
            let forgotten = self.update_drafts(review_unit, |_, _| Ok(()));
            self.report(forgotten);
        }
        (self.publish)(Event::Posted(ui_events::ThreadPostFinished {
            review_unit: review_unit.clone(),
            message_id: message_id.clone(),
            result,
        }));
        if let Some(round) = round {
            (self.publish)(Event::RoundMessage(crate::RoundMessage {
                review_unit: review_unit.clone(),
                round,
                message: message_id,
            }));
        }
        if posted && let Err(error) = self.schedule(review_unit, false) {
            self.undelivered(Some(review_unit), error);
        }
        // A comment that wakes an agent waits for the next poll, which picks it.
        if !self.notifications.contains_key(review_unit) {
            self.publish_comments(review_unit, None);
        }
    }

    /// Publishes the new review comments of `unit` on the bus, to the agent in the pane `to`.
    fn publish_comments(&mut self, unit: &ReviewUnit, to: Option<&PaneId>) {
        let comments = self.unpublished.remove(unit).unwrap_or_default();
        let Some(bus) = &self.bus else {
            return;
        };
        for comment in comments {
            bus.publish(UserWords {
                to: to.cloned(),
                ..comment
            });
        }
    }

    fn load(&mut self, unit: &ReviewUnit) -> Result<ReviewThreads, String> {
        let book = self
            .store
            .load_threads(unit)
            .map_err(|error| error.to_string())?;
        self.books.insert(unit.clone(), book.clone());
        Ok(book)
    }

    fn update<T>(
        &mut self,
        unit: &ReviewUnit,
        update: impl FnOnce(&mut ReviewThreads) -> Result<T, String>,
    ) -> Result<T, String> {
        let (result, book) = self
            .store
            .update_threads(unit, update)
            .map_err(|error| error.to_string())?;
        let previous = self.books.insert(book.review_unit.clone(), book.clone());
        if previous.as_ref() != Some(&book) {
            let drafts = self.drafts_to_restore(&book.review_unit);
            (self.publish)(Event::Loaded(ui_events::ReviewThreadsLoaded {
                review_unit: book.review_unit.clone(),
                result: Ok(book),
                drafts,
            }));
        }
        Ok(result)
    }

    /// Change the saved drafts. The posted threads are never rewritten for a draft.
    fn update_drafts(
        &mut self,
        unit: &ReviewUnit,
        update: impl FnOnce(&mut SavedDrafts, &ReviewThreads) -> Result<(), String>,
    ) -> Result<(), String> {
        self.store
            .update_drafts(unit, update)
            .map(|((), _)| ())
            .map_err(|error| error.to_string())
    }

    fn refresh_target(&mut self) {
        for unit in self.books.keys().cloned().collect::<Vec<_>>() {
            self.resume(&unit);
        }
    }

    fn cancel_wakeups(&mut self, unit: &ReviewUnit) {
        self.notifications.remove(unit);
        self.wakeups.retain(|token, _| {
            self.access
                .get(token)
                .is_some_and(|access| &access.review_unit != unit)
        });
    }

    fn schedule(&mut self, unit: &ReviewUnit, retry: bool) -> Result<(), String> {
        if !self.books.contains_key(unit) {
            return Err("The review history is not loaded".into());
        }
        if !self.waits_for_agent(unit) {
            return Ok(());
        }
        if !self.available {
            return Err("The comment is saved, but the reviewer MCP server is unavailable".into());
        }
        let notification = self.notifications.entry(unit.clone()).or_default();
        notification.retry |= retry;
        Ok(())
    }

    fn grant(&mut self, unit: &ReviewUnit, agent: Agent) -> Result<Access, String> {
        for access in self.access.values() {
            if access.review_unit == *unit && access.matches_agent(&*self.port, &agent)? {
                return Ok(access.clone());
            }
        }
        let access = Access::new(unit.clone(), agent, &*self.port)?;
        self.cancel_wakeups(unit);
        self.access
            .retain(|_, previous| previous.review_unit != *unit);
        self.access.insert(access.token.clone(), access.clone());
        Ok(access)
    }

    /// This tab's review pane, as the recipient of the pending comments of `unit`.
    fn recipient(&self, unit: &ReviewUnit) -> Recipient<'_> {
        const NONE: &BTreeMap<ThreadId, u64> = &BTreeMap::new();
        Recipient {
            tab: &self.target.tab_id().0,
            retried: self.retried.get(unit).unwrap_or(NONE),
        }
    }

    /// Whether a pending comment of `unit` is for this tab's agent: one this tab posted, or one
    /// of a thread the reviewer retried here. Another tab's comment, which may come from the
    /// review pane of another workspace of the repository, waits for that tab's reviewer.
    fn waits_for_agent(&self, unit: &ReviewUnit) -> bool {
        self.books
            .get(unit)
            .is_some_and(|book| book.has_new_messages_for(&self.recipient(unit)))
    }

    /// Bring the comments of `thread` posted so far to this tab's agent, whichever tab posted
    /// them; a comment posted later waits for its own tab.
    fn release(&mut self, unit: &ReviewUnit, thread: ThreadId) {
        let Some(through) = self
            .books
            .get(unit)
            .and_then(|book| book.thread(&thread)?.last_comment())
            .map(Message::sequence)
        else {
            return;
        };
        self.retried
            .entry(unit.clone())
            .or_default()
            .insert(thread, through);
    }

    fn resume(&mut self, unit: &ReviewUnit) {
        if self.waits_for_agent(unit) {
            let result = self.schedule(unit, false);
            self.report(result);
        }
    }

    fn request_wakeup(&mut self, access: &Access, retry: bool) {
        let unit = &access.review_unit;
        let Some(sequence) = self
            .books
            .get(unit)
            .and_then(|book| book.pending_comment_sequence_for(&self.recipient(unit)))
        else {
            return;
        };
        self.wakeups
            .entry(access.token.clone())
            .or_default()
            .request(sequence, retry);
    }

    /// Serves an agent's call with `access`, which names the agent's session `session` if the
    /// agent names one, as Codex does.
    fn request(
        &mut self,
        access: &str,
        operation: &Operation,
        session: Option<&str>,
    ) -> Result<Response, String> {
        let access =
            self.access.get(access).cloned().ok_or(
                "Unknown review access value; use the value in the latest reviewer wakeup",
            )?;
        let agent = access.current_agent(&*self.port)?;
        if let (Some(hooks), Some(session)) = (self.courier.hooks(), session) {
            hooks.name_session(&agent.pane_id, session);
        }
        let book = self.load(&access.review_unit)?;
        match operation {
            Operation::SubmitQuestion(_)
            | Operation::SubmitConclusion(_)
            | Operation::ReviseQuestion(_) => {
                Err("Explore requests belong to the interview owner".into())
            }
            Operation::ListThreads => Ok(Response::Threads(book.threads().to_vec())),
            Operation::GetThread(id) => {
                let thread = book
                    .thread(id)
                    .cloned()
                    .ok_or("The review thread no longer exists")?;
                Ok(Response::Threads(vec![thread]))
            }
            Operation::GetNewMessages => Ok(Response::NewMessages(
                book.new_messages_for(&self.recipient(&access.review_unit)),
            )),
            Operation::Reply(post) => {
                let id = self.update(&access.review_unit, |book| book.answer((**post).clone()))?;
                if !self.waits_for_agent(&access.review_unit)
                    && let Some(wakeup) = self.wakeups.get_mut(&access.token)
                {
                    wakeup.answered();
                }
                Ok(Response::Posted(id))
            }
        }
    }

    fn observe(&mut self, event: HerdrEvent) {
        match event {
            HerdrEvent::AgentDetected {
                pane_id,
                released: true,
                ..
            } => {
                // An agent can resume in the same pane after MCP setup.
                // Keep its grant; every request still checks the live identity.
                self.interrupt_wakeups(&pane_id);
                self.refresh_target();
            }
            HerdrEvent::AgentDetected {
                pane_id,
                released: false,
                ..
            } => {
                self.resume_access(&pane_id);
                self.refresh_target();
            }
            // The poll after each input sends the prompts that waited for the session.
            HerdrEvent::AgentSessionChanged(_) | HerdrEvent::PaneFocused(_) => {}
        }
    }

    fn resume_access(&mut self, pane: &PaneId) {
        let valid = self
            .access
            .values()
            .filter(|access| {
                &access.agent.pane_id == pane
                    && !self.wakeups.contains_key(&access.token)
                    && access.current_agent(&*self.port).is_ok()
            })
            .cloned()
            .collect::<Vec<_>>();
        for access in valid {
            self.request_wakeup(&access, false);
        }
    }

    fn interrupt_wakeups(&mut self, pane: &PaneId) {
        for (token, access) in &self.access {
            if access.agent.pane_id == *pane
                && let Some(wakeup) = self.wakeups.get_mut(token)
            {
                wakeup.interrupted();
            }
        }
    }

    fn poll(&mut self) {
        self.prompts.poll(&*self.port, &self.courier);
        for unit in self.notifications.keys().cloned().collect::<Vec<_>>() {
            match self.prepare_notification(&unit) {
                Ok(woken) => self.publish_comments(&unit, woken.as_ref()),
                Err(error) => {
                    self.publish_comments(&unit, None);
                    self.notifications.remove(&unit);
                    self.undelivered(Some(&unit), error);
                }
            }
        }
        let tokens = self
            .wakeups
            .iter()
            .filter(|(_, wakeup)| wakeup.needs_poll())
            .map(|(token, _)| token.clone())
            .collect::<Vec<_>>();
        for token in tokens {
            if let Err(error) = self.notify(&token) {
                self.wakeups.remove(&token);
                let unit = self
                    .access
                    .get(&token)
                    .map(|access| access.review_unit.clone());
                self.undelivered(unit.as_ref(), error);
            }
        }
    }

    /// Grants the agent the reviewer wakes for the pending comments of `unit` its access, and
    /// requests its wakeup; returns its pane, or `None` when no comment waits for it.
    fn prepare_notification(&mut self, unit: &ReviewUnit) -> Result<Option<PaneId>, String> {
        let retry = self
            .notifications
            .get(unit)
            .expect("pending notification")
            .retry;
        if !self.waits_for_agent(unit) {
            self.cancel_wakeups(unit);
            return Ok(None);
        }
        let agent = self
            .target
            .resolve(&*self.port)
            .map_err(|error| error.to_string())?
            .ok_or("Focus an implementation agent to receive the saved comments")?;
        let pane = agent.pane_id.clone();
        let access = self.grant(unit, agent)?;
        self.notifications.remove(unit);
        self.request_wakeup(&access, retry);
        Ok(Some(pane))
    }

    fn notify(&mut self, token: &str) -> Result<(), String> {
        let Some(access) = self.access.get(token) else {
            return Ok(());
        };
        if !self.waits_for_agent(&access.review_unit) {
            return Ok(());
        }
        let current = self
            .target
            .resolve(&*self.port)
            .map_err(|error| error.to_string())?
            .ok_or("Focus an implementation agent to receive the saved comments")?;
        if !access.matches_agent(&*self.port, &current)? {
            let unit = access.review_unit.clone();
            self.wakeups.remove(token);
            return self.schedule(&unit, false);
        }
        let prompt = access.prompt();
        let Some(wakeup) = self.wakeups.get_mut(token) else {
            return Ok(());
        };
        if wakeup.needs_poll() {
            let through = wakeup.sent();
            // One on its way, or waiting behind one that waits.
            let notice = wakeup.held().map(|(wait, through)| {
                WakeupNotice::Waiting(WakeupWait {
                    through,
                    reason: wait.to_string(),
                })
            });
            let unit = access.review_unit.clone();
            self.report_wakeup(Some(&unit), notice);
            let inputs = self.inputs.clone();
            let token = token.to_owned();
            let turns = self.turns.clone();
            let sent = prompt.clone();
            let waits = {
                let (inputs, token) = (self.inputs.clone(), token.clone());
                move |wait| {
                    let _ = inputs.send(Input::Notification {
                        token: token.clone(),
                        through,
                        news: NotificationNews::Waits(wait),
                    });
                }
            };
            self.courier
                .send_telling_waits(current, prompt, None, waits, move |result| {
                    if let Some(turns) = &turns {
                        let turn = CommentsTurn {
                            kind: "comments",
                            access: &token,
                            text: &sent,
                        };
                        turns.record(&turn, result.as_ref().err().map(ToString::to_string));
                    }
                    let _ = inputs.send(Input::Notification {
                        token,
                        through,
                        news: NotificationNews::Outcome(result),
                    });
                });
        }
        Ok(())
    }

    fn report(&self, result: Result<(), String>) {
        if let Err(error) = result {
            (self.publish)(Event::Error(error));
        }
    }
}

#[cfg(test)]
#[path = "state.tests.rs"]
mod tests;
