mod agent;
mod cancel;
mod diagram;
mod dispatch;
mod held_access;
mod implementation;
mod mark_tally;
mod marks;
mod page;
mod page_actions;
mod page_save;
mod past;
mod quiz;
mod records;
mod restore;
mod run_ahead;
mod start_block;
mod submission;
mod turn;
mod turn_files;
mod turn_log;
mod unreviewed;
mod unreviewed_diffs;
mod words;

pub use past::{PastRoundsRefresh, SavedRoundsReader};
pub use run_ahead::{RunAheadInput, TALK_QUIET};

use std::sync::Arc;
use vision_turns::TurnLog;

use component_core::ApplicationEventSender;
use herdr_client::protocol::{AgentPort, AgentTarget};
use review_explore::{Command, Comparison, Exploration, ExploreRound, TurnRequest, ViewSave};
use review_explore_page::{CommandRefusal, CommandReply, RoundPublisher};
use review_repository::repository::Repository;
use review_state::ReviewTracker;
use review_store::ReviewStore;
use review_thread_service::{PinnedAgent, PromptCancellation, PromptSender};
use review_types::ReviewUnit;

#[derive(Debug)]
pub enum Input {
    Command(Command),
    Submission(Box<review_mcp::Request>),
    PromptFinished {
        event: Box<ui_events::ExploreFinished>,
        attempt: String,
    },
    StorageChanged,
    Page {
        command: review_explore_page::PageCommand,
        reply: review_explore_page::CommandReply,
    },
    RunAhead(RunAheadInput),
    RoundMessage(review_thread_service::RoundMessage),
}

#[derive(Clone)]
pub struct Inbox(Arc<dyn Fn(Input) + Send + Sync>);

impl Inbox {
    pub fn new(deliver: impl Fn(Input) + Send + Sync + 'static) -> Self {
        Self(Arc::new(deliver))
    }

    pub fn deliver(&self, input: Input) {
        (self.0)(input);
    }
}

pub struct Collaborators {
    pub repository: Repository,
    pub store: ReviewStore,
    pub tracker: Arc<ReviewTracker>,
    pub agents: Arc<dyn AgentPort>,
    pub target: AgentTarget,
    pub prompts: PromptSender,
    pub events: ApplicationEventSender,
    pub inbox: Inbox,
    pub turns: Option<TurnLog>,
    pub page: RoundPublisher,
    pub forks: Arc<dyn review_run_ahead::ForkHost>,
    pub talk_quiet: std::time::Duration,
    pub hooks: Option<Arc<agent_hooks::AgentHooks>>,
    pub references: Option<change_references_finder::ReferencesFinder>,
}

pub struct ExploreSession {
    repository: Repository,
    tracker: Arc<ReviewTracker>,
    rounds: records::SavedRounds,
    agents: Arc<dyn AgentPort>,
    target: AgentTarget,
    prompts: PromptSender,
    events: ApplicationEventSender,
    inbox: Inbox,
    turns: Option<TurnLog>,
    page: RoundPublisher,
    hooks: Option<Arc<agent_hooks::AgentHooks>>,
    citations: page::PageCitations,
    earlier_citations: page::PageCitations,
    marks: review_explore_tally::ChangeMarks,
    marks_snapshot: Option<review_repository::repository::Snapshot>,
    markable: mark_tally::MarkableCache,
    prompt_diverged: Vec<String>,
    prompted_diverged: std::collections::BTreeMap<String, Vec<String>>,
    diffs: Option<unreviewed_diffs::UnreviewedDiffs>,
    turn_files: Option<turn_files::TurnFiles>,
    start_block: Option<review_explore::StartBlock>,
    run_ahead: run_ahead::RunAheadState,
    state: State,
}

#[derive(Debug, Default)]
struct State {
    comparison: Option<Arc<Comparison>>,
    round: Option<ExploreRound>,
    loaded_unit: Option<ReviewUnit>,
    historical: bool,
    storage_error: Option<String>,
    access: String,
    held: held_access::HeldAccess,
    grants_held: bool,
    last_view: Option<ViewSave>,
    pending: Option<(String, String)>,
    dismissed: Option<String>,
    start: Start,
    agent: Option<PinnedAgent>,
    prompt: Option<PromptCancellation>,
    implementation: Option<implementation::SendingImplementation>,
}

#[derive(Debug)]
enum Start {
    Idle { offer: String },
    Starting { start: String, started_at_ms: u64 },
    Failed { failure: String, offer: String },
}

impl Default for Start {
    fn default() -> Self {
        Self::Idle {
            offer: uuid::Uuid::new_v4().to_string(),
        }
    }
}

impl Start {
    fn offered(&self) -> Option<&str> {
        match self {
            Self::Idle { offer } | Self::Failed { offer, .. } => Some(offer),
            Self::Starting { .. } => None,
        }
    }

    fn starting(&self) -> Option<&str> {
        match self {
            Self::Starting { start, .. } => Some(start),
            _ => None,
        }
    }

    fn begin(&mut self) {
        if let Some(offer) = self.offered() {
            *self = Self::Starting {
                start: offer.to_owned(),
                started_at_ms: review_explore::now_ms(),
            };
        }
    }

    fn settle(&mut self) {
        if self.starting().is_some() {
            *self = Self::default();
        }
    }
}

impl State {
    fn start_failed(&mut self, error: &dyn std::fmt::Display) {
        self.start = Start::Failed {
            failure: error.to_string(),
            offer: uuid::Uuid::new_v4().to_string(),
        };
    }

    fn pend(&mut self, request: &TurnRequest) {
        self.pending = Some((request.instance.clone(), request.request.clone()));
    }

    fn is_pending(&self, instance: &str, request: &str) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|(pending_instance, pending)| {
                pending_instance == instance && pending == request
            })
    }

    fn renew_access(&mut self) {
        self.access = uuid::Uuid::new_v4().to_string();
        self.grants_held = false;
    }

    fn grants(&self, access: &str) -> bool {
        let held = self.grants_held.then(|| self.held.held()).flatten();
        held.as_deref().unwrap_or(&self.access) == access
    }
}

impl Drop for ExploreSession {
    fn drop(&mut self) {
        self.run_ahead_close();
    }
}

impl std::fmt::Debug for ExploreSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ExploreSession")
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

impl ExploreSession {
    pub fn new(collaborators: Collaborators) -> Self {
        let Collaborators {
            repository,
            store,
            tracker,
            agents,
            target,
            prompts,
            events,
            inbox,
            turns,
            page,
            forks,
            talk_quiet,
            hooks,
            references,
        } = collaborators;
        Self {
            repository,
            tracker,
            marks: review_explore_tally::ChangeMarks::new(store.clone()),
            marks_snapshot: None,
            markable: mark_tally::MarkableCache::default(),
            prompt_diverged: Vec::new(),
            prompted_diverged: std::collections::BTreeMap::new(),
            rounds: records::SavedRounds::new(store).with_references(references),
            agents,
            target,
            prompts,
            events,
            inbox,
            turns,
            page,
            citations: page::PageCitations::default(),
            earlier_citations: page::PageCitations::default(),
            diffs: None,
            turn_files: None,
            start_block: None,
            run_ahead: run_ahead::RunAheadState::new(forks, talk_quiet),
            hooks,
            state: State::default(),
        }
    }

    pub fn handle(&mut self, input: Input) {
        match input {
            Input::Command(command) => self.command(command),
            Input::Submission(request) => self.submission(*request),
            Input::PromptFinished { event, attempt } => self.prompt_finished(*event, &attempt),
            Input::StorageChanged => self.storage_changed(),
            Input::Page { command, reply } => self.page_command(command, reply),
            Input::RunAhead(input) => self.run_ahead_input(input),
            Input::RoundMessage(message) => self.run_ahead_round_message(&message),
        }
        self.run_ahead_reconcile();
        self.publish_page();
    }

    pub fn runs_round(&self) -> bool {
        self.state.round.is_some()
    }

    pub fn checkpoint_changed(&mut self, unit: &ReviewUnit) {
        if self.state.loaded_unit.as_ref() == Some(unit) {
            return;
        }
        self.run_ahead_discard(review_run_ahead::DiscardReason::ReviewChanged);
        self.state = State {
            loaded_unit: Some(unit.clone()),
            ..State::default()
        };
        self.open();
        self.run_ahead_reconcile();
        self.publish_page();
    }

    fn command(&mut self, command: Command) {
        match command {
            Command::Start => self.start(),
            Command::Reset => self.reset(),
            Command::SaveView(view) => self.save_view(*view),
            Command::Turn(request) => {
                let _ = self.deliver_turn(*request, None);
            }
            Command::Retry(request) => {
                let _ = self.retry(*request);
            }
            Command::Implement(request) => {
                let _ = self.implement(request);
            }
            Command::CancelImplementation => self.state.implementation = None,
            Command::CancelAnswer(answer) => {
                let _ = self.cancel_answer(answer, cancel::Cancel::Latest);
            }
            Command::Cancel => self.stop_waiting(),
        }
    }

    fn stop_waiting(&mut self) {
        let waited = self.run_ahead_stop_waiting();
        self.cancel_record();
        self.state.start.settle();
        self.state.renew_access();
        self.state.prompt = None;
        self.state.pending = None;
        self.state.implementation = None;
        if let Some(request) = waited {
            let _ = self.retry(request);
        }
    }

    fn start(&mut self) {
        if self.state.storage_error.is_some() || !self.cancel_record() {
            let error = self
                .state
                .storage_error
                .clone()
                .unwrap_or_else(|| "Explore could not save its state".into());
            self.state.start_failed(&error);
            let _ = self
                .events
                .send(ui_events::ExploreCaptured { result: Err(error) });
            return;
        }
        self.state.prompt = None;
        self.state.implementation = None;
        self.state.pending = None;
        let result = self.capture();
        match &result {
            Ok(comparison) => self.begin_start(comparison.clone()),
            Err(error) => self.state.start_failed(error),
        }
        let _ = self.events.send(ui_events::ExploreCaptured {
            result: result.map_err(|error| error.to_string()),
        });
    }

    pub fn start_from_page(
        &mut self,
        choices: review_explore::RoundChoices,
        start: &str,
        reply: CommandReply,
    ) -> Option<TurnRequest> {
        let refusal = if self.state.round.is_none() && self.state.start.starting() == Some(start) {
            Some(CommandRefusal::AlreadyApplied)
        } else if self.state.round.is_some() || self.state.start.offered() != Some(start) {
            Some(CommandRefusal::Stale)
        } else if let Some(block) = self.start_block {
            Some(CommandRefusal::Failed(block.reason().into()))
        } else {
            self.state.storage_error.clone().map(CommandRefusal::Failed)
        };
        if let Some(refusal) = refusal {
            reply.send(Err(refusal));
            return None;
        }
        self.state.start.begin();
        self.publish_page();
        reply.send(Ok(()));
        let _ = self.events.send(ui_events::ExplorePageStart(Ok(())));
        let kickoff = self.capture().and_then(|comparison| {
            self.begin_start(comparison.clone());
            let mut exploration = Exploration::new(comparison);
            exploration.choose(choices);
            exploration.writing = self.rounds.next_writing()?;
            exploration.request(None, None)
        });
        match kickoff {
            Ok(kickoff) => Some(kickoff),
            Err(error) => {
                let _ = self
                    .events
                    .send(ui_events::ExplorePageStart(Err(error.to_string())));
                self.state.start_failed(&error);
                self.publish_page();
                None
            }
        }
    }

    fn begin_start(&mut self, comparison: Arc<Comparison>) {
        self.run_ahead_discard(review_run_ahead::DiscardReason::NewRound);
        self.state.comparison = Some(comparison);
        self.state.agent = None;
        self.state.round = None;
        self.state.historical = false;
        self.state.start.begin();
        self.state.renew_access();
        self.state.held = held_access::HeldAccess::default();
    }

    fn reset(&mut self) {
        self.run_ahead_discard(review_run_ahead::DiscardReason::Reset);
        self.cancel_record();
        let closed = self.state.round.as_ref().map_or(Ok(()), |round| {
            self.rounds.close(
                &round.exploration.comparison.checkpoint.review_unit,
                &round.exploration.instance,
            )
        });
        let dismissed = self.state.loaded_unit.as_ref().and_then(|unit| {
            let history = self.rounds.history(unit).ok()?;
            history.restorable().map(str::to_owned)
        });
        self.state = State {
            loaded_unit: self.state.loaded_unit.take(),
            storage_error: self.state.storage_error.take(),
            dismissed,
            ..State::default()
        };
        self.state.renew_access();
        if let Err(error) = closed {
            self.state.storage_error = Some(error.to_string());
            let _ = self
                .events
                .send(ui_events::ExploreStorageFailed(error.to_string()));
        }
    }

    fn capture(&mut self) -> eyre::Result<Arc<Comparison>> {
        let review_repository::repository::PollResult::Complete(snapshot) =
            self.repository.poll()?
        else {
            eyre::bail!("Repository comparison is not ready; retry Start");
        };
        if let Some(block) = self.refresh_start_block(&snapshot) {
            eyre::bail!("{block}");
        }
        Ok(Arc::new(Comparison::prepare(&self.repository, &snapshot)?))
    }

    fn cancel_record(&mut self) -> bool {
        let Some(round) = &self.state.round else {
            return true;
        };
        if self.state.historical {
            return true;
        }
        if let Err(error) = self.rounds.update(
            &round.exploration.comparison.checkpoint.review_unit,
            &round.exploration.instance,
            |round| {
                round.exploration.cancel();
                Ok(())
            },
        ) {
            self.state.storage_error = Some(error.to_string());
            let _ = self
                .events
                .send(ui_events::ExploreStorageFailed(error.to_string()));
            return false;
        }
        true
    }
}

#[cfg(test)]
#[path = "lib.tests.rs"]
mod tests;
