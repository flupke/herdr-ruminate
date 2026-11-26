use std::ops::ControlFlow;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};

use agent_hooks::AgentHooks;
use change_references_finder::ReferencesFinder;
use component_core::{ApplicationEventSender, EventEnvelope};
use crossbeam_channel::{Receiver as EventReceiver, Sender as EventSender};
use herdr_client::protocol::{AgentHost, AgentTarget, HerdrEvent, PaneId};
use linear_issues::LinearSettings;
use review_explore_page::RoundPublisher;
use review_explore_page_host::{Browser, PageOpener};
use review_explore_page_opening::{PageNotOpened, PageScreen};
use review_explore_session::{self as explore_session, ExploreSession};
use review_repository::repository::Repository;
use review_significance::JevClassifier;
use review_state::ReviewTracker;
use review_store::ReviewStore;
use review_thread_service as comments;
use review_ui::{
    Action, DocumentAction, DocumentLoad, ExplorePageAction, LspAction, RepositoryAction,
    SettingsAction, TerminalAction, Theme,
};

use super::actions::ActionExecutors;
use super::document;
use super::highlighting;
use super::page_sharing::PageSharing;
use super::page_threads::PageThreads;
use super::revision_progress::HistoryProgress;
use super::route::WorkerStopped;
use super::worker::{Worker, WorkerCommand};
use crate::watcher::SourceWatchRequests;

pub(super) struct Setup {
    pub(super) repository: Repository,
    pub(super) store: ReviewStore,
    pub(super) target: AgentTarget,
    pub(super) agents: Arc<dyn AgentHost>,
    pub(super) endpoint: Result<review_mcp::Endpoint, String>,
    pub(super) theme: Theme,
    pub(super) jev: JevClassifier,
    pub(super) linear: Option<LinearSettings>,
    pub(super) turns: Option<vision_turns::TurnLog>,
    pub(super) bus: Option<Result<PathBuf, String>>,
    pub(super) source_watches: Option<SourceWatchRequests>,
    pub(super) page: RoundPublisher,
    pub(super) page_threads: Arc<PageThreads>,
    pub(super) hook_directory: Option<agent_hooks::HookDirectory>,
    pub(super) run_ahead: RunAheadSetup,
}

pub(super) struct RunAheadSetup {
    pub(super) tools: claude_fork::ForkTools,
    pub(super) log: Option<PathBuf>,
    pub(super) waits: review_run_ahead::ForkWaits,
    pub(super) talk_quiet: std::time::Duration,
}

impl RunAheadSetup {
    fn forks(
        self,
        agents: &Arc<dyn AgentHost>,
        hooks: Result<Arc<AgentHooks>, String>,
    ) -> Arc<agent_forks::AgentForks> {
        let log = review_run_ahead::RunAheadLog::at(self.log);
        let codex = codex_fork::CodexForks::new(
            Arc::clone(agents),
            log.clone(),
            self.waits,
            hooks.as_ref().ok().cloned(),
        );
        let claude =
            claude_fork::ClaudeForks::new(Arc::clone(agents), self.tools, log, self.waits, hooks);
        Arc::new(agent_forks::AgentForks::new(claude, codex))
    }
}

fn listen_to_hooks(
    directory: Option<&agent_hooks::HookDirectory>,
) -> Result<Arc<AgentHooks>, String> {
    let directory = directory.ok_or(
        "cannot listen to the agents' hooks: XDG_RUNTIME_DIR, or both HERDR_SOCKET_PATH and \
         HC_TERMINAL_SOCKET, are not set",
    )?;
    AgentHooks::listen(directory)
        .map(Arc::new)
        .map_err(|error| format!("cannot listen to the agents' hooks: {error}"))
}

pub(super) struct Outputs {
    pub(super) background: EventSender<EventEnvelope>,
    pub(super) interactive: EventSender<EventEnvelope>,
}

pub(super) struct Effects {
    repository_root: PathBuf,
    target: AgentTarget,
    store: ReviewStore,
    source_watches: Option<SourceWatchRequests>,
    commands: Sender<WorkerCommand>,
    documents: Sender<document::Command>,
    page_opener: Option<PageOpener>,
    page_threads: Arc<PageThreads>,
    page_sharing: Option<PageSharing>,
    messages: ApplicationEventSender,
    front: Option<FrontWorkers>,
    lsp: review_lsp::Worker,
    threads: Vec<JoinHandle<()>>,
}

struct FrontWorkers {
    comments: comments::Worker,
    search: text_search::Worker,
    highlighting: highlighting::Worker,
}

type TerminalExecutor<'a> = dyn FnMut(TerminalAction) -> eyre::Result<ControlFlow<()>> + 'a;

impl Effects {
    pub(super) fn start(setup: Setup, outputs: &Outputs) -> Self {
        let Setup {
            repository,
            store,
            target,
            agents,
            endpoint,
            theme,
            jev,
            linear,
            turns,
            bus,
            source_watches,
            page,
            page_threads,
            hook_directory,
            run_ahead,
        } = setup;
        let messages = ApplicationEventSender::new(outputs.background.clone());
        let tracker = Arc::new(ReviewTracker::new(repository.clone(), store.clone()));
        let (commands, command_receiver) = mpsc::channel();
        let (documents, document_thread) =
            start_documents(repository.clone(), Arc::clone(&tracker), messages.clone());
        let hooks = listen_to_hooks(hook_directory.as_ref());
        let comments = start_comments(
            &store,
            Arc::clone(&agents),
            hooks.as_ref().ok().cloned(),
            target.clone(),
            endpoint,
            &commands,
            thread_events(&commands, messages.clone(), Arc::clone(&page_threads)),
        );
        share_comments(&comments, turns.as_ref(), bus);
        let talk_quiet = run_ahead.talk_quiet;
        let explore_hooks = hooks.as_ref().ok().cloned();
        let forks = run_ahead.forks(&agents, hooks);
        let explore = ExploreSession::new(explore_session::Collaborators {
            repository: repository.clone(),
            store: store.clone(),
            tracker: Arc::clone(&tracker),
            agents,
            target: target.clone(),
            prompts: comments.prompt_sender(),
            events: messages.clone(),
            inbox: inbox_for(commands.clone()),
            turns,
            page,
            forks,
            talk_quiet,
            hooks: explore_hooks,
            references: Some(ReferencesFinder::new(linear.clone())),
        });
        let mut worker = Worker {
            repository: repository.clone(),
            tracker,
            store: store.clone(),
            snapshot: None,
            commands: commands.clone(),
            explore,
            jev,
            linear,
            auto_review: None,
            held_kickoff: None,
            documents: documents.clone(),
            revision_progress: HistoryProgress::default(),
        };
        let worker_messages = messages.clone();
        let worker_thread = thread::spawn(move || {
            worker.run(&command_receiver, &worker_messages);
            let _ = worker_messages.send(WorkerStopped);
        });
        let highlights = outputs.background.clone();
        let search_results = outputs.interactive.clone();
        Self {
            repository_root: repository.root().to_owned(),
            target,
            store,
            source_watches,
            commands,
            documents,
            page_opener: None,
            page_threads,
            page_sharing: None,
            messages,
            front: Some(FrontWorkers {
                comments,
                search: text_search::Worker::start(move |results| {
                    let _ = search_results.send(EventEnvelope::new(results));
                }),
                highlighting: highlighting::Worker::start(
                    syntax_highlighting::SyntaxHighlighter::new(theme.syntax, theme.palette.text),
                    move |result| {
                        let _ = highlights.send(EventEnvelope::new(result));
                    },
                ),
            }),
            lsp: review_lsp::Worker::start(repository.root().to_owned()),
            threads: vec![worker_thread, document_thread],
        }
    }

    pub(super) fn perform_all(
        &self,
        actions: Vec<Action>,
        terminal: &mut TerminalExecutor<'_>,
    ) -> eyre::Result<ControlFlow<()>> {
        Performer {
            effects: self,
            terminal,
        }
        .run_all(actions)
    }

    pub(super) fn refresh(&self) -> eyre::Result<()> {
        self.commands.send(WorkerCommand::Poll)?;
        Ok(())
    }

    pub(super) fn agent_focused(&mut self, pane_id: &PaneId) {
        self.target.observe_focus(pane_id);
        self.front()
            .comments
            .send(comments::Command::ActiveAgentChanged);
    }

    pub(super) fn observe(&self, event: &HerdrEvent) {
        self.front()
            .comments
            .send(comments::Command::Observe(event.clone()));
    }

    fn front(&self) -> &FrontWorkers {
        self.front.as_ref().expect("front workers run until drop")
    }

    pub(super) fn share_page(&mut self, sharing: PageSharing) {
        self.page_sharing = Some(sharing);
    }

    pub(super) fn open_page_with(&mut self, opener: PageOpener) {
        self.page_opener = Some(opener);
    }

    pub(super) fn explore_inbox(&self) -> explore_session::Inbox {
        inbox_for(self.commands.clone())
    }

    pub(super) fn page_thread_sender(&self) -> review_explore_page::ThreadSender {
        self.page_threads
            .sender(self.front().comments.thread_commands())
    }

    pub(super) fn lsp_file_changes(&self) -> review_lsp::FileChangeSender {
        self.lsp.file_change_sender()
    }

    pub(super) fn lsp_events(&self) -> EventReceiver<review_lsp::Event> {
        self.lsp.event_receiver()
    }

    fn load_documents(&self, load: DocumentLoad) -> eyre::Result<()> {
        match load {
            DocumentLoad::Diffs {
                review_checkpoint,
                paths,
            } => {
                for path in paths {
                    self.documents
                        .send(document::Command::Load(DocumentLoad::Diff {
                            review_checkpoint: review_checkpoint.clone(),
                            path,
                        }))?;
                }
            }
            DocumentLoad::Source {
                snapshot_id,
                mut location,
                mode,
            } => {
                location.path = self.resolve(location.path);
                self.documents
                    .send(document::Command::Load(DocumentLoad::Source {
                        snapshot_id,
                        location,
                        mode,
                    }))?;
            }
            load @ DocumentLoad::Diff { .. } => {
                self.documents.send(document::Command::Load(load))?;
            }
        }
        Ok(())
    }

    fn open_page(&self, screen: PageScreen) {
        let Some(opener) = self.page_opener.clone() else {
            let _ = self
                .messages
                .send(ui_events::ExplorePageNotOpened(PageNotOpened {
                    url: None,
                    reason: "the Explore page is not served".into(),
                }));
            return;
        };
        let messages = self.messages.clone();
        thread::spawn(move || {
            if let Err(failure) = opener.open(screen) {
                let _ = messages.send(ui_events::ExplorePageNotOpened(failure));
            }
        });
    }

    fn open_link(&self, url: String) {
        let messages = self.messages.clone();
        thread::spawn(move || {
            if let Err(reason) = Browser::from_env().open(&url) {
                let _ = messages.send(ui_events::ToastRequested {
                    text: format!("Cannot open {url}: {reason}"),
                    kind: toasts::ToastKind::Error,
                });
            }
        });
    }

    fn share_round(&self, on: bool) {
        use review_explore_page_host::TunnelState;
        match (&self.page_sharing, on) {
            (Some(sharing), true) => sharing.open_tunnel(),
            (Some(sharing), false) => sharing.close_tunnel(),
            (None, on) => {
                let state = if on {
                    TunnelState::Failed("the Explore page is not served".into())
                } else {
                    TunnelState::Off
                };
                let _ = self.messages.send(ui_events::ExplorePageTunnel(state));
            }
        }
    }

    fn resolve(&self, path: PathBuf) -> PathBuf {
        if path.is_relative() {
            self.repository_root.join(path)
        } else {
            path
        }
    }

    #[cfg(test)]
    pub(super) fn hold_repository_work(&self) -> EventSender<()> {
        let (hold, release) = crossbeam_channel::bounded(0);
        self.commands
            .send(WorkerCommand::Hold(release))
            .expect("the repository worker is running");
        hold
    }

    #[cfg(test)]
    pub(super) fn flush(&self) {
        self.hold_repository_work()
            .send(())
            .expect("the repository worker is running");
        self.front().comments.prompt_sender().flush();
    }

    #[cfg(test)]
    pub(super) fn replace_highlighting(&mut self, highlighting: highlighting::Worker) {
        self.front.as_mut().unwrap().highlighting = highlighting;
    }
}

impl Drop for Effects {
    fn drop(&mut self) {
        drop(self.front.take());
        let _ = self.commands.send(WorkerCommand::Quit);
        let _ = self.documents.send(document::Command::Quit);
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

fn inbox_for(commands: Sender<WorkerCommand>) -> explore_session::Inbox {
    explore_session::Inbox::new(move |input| {
        let _ = commands.send(WorkerCommand::Explore(input));
    })
}

fn start_documents(
    repository: Repository,
    tracker: Arc<ReviewTracker>,
    messages: ApplicationEventSender,
) -> (Sender<document::Command>, JoinHandle<()>) {
    let (documents, receiver) = mpsc::channel();
    let mut worker = document::DocumentWorker {
        repository,
        tracker,
        snapshot: None,
    };
    let thread = thread::spawn(move || {
        worker.run(&receiver, &messages);
        let _ = messages.send(WorkerStopped);
    });
    (documents, thread)
}

fn start_comments(
    store: &ReviewStore,
    agents: Arc<dyn AgentHost>,
    hooks: Option<Arc<AgentHooks>>,
    target: AgentTarget,
    endpoint: Result<review_mcp::Endpoint, String>,
    commands: &Sender<WorkerCommand>,
    events: impl Fn(comments::Event) + Send + 'static,
) -> comments::Worker {
    let commands = commands.clone();
    comments::Worker::start(
        store.clone(),
        agents,
        hooks,
        target,
        endpoint,
        move |request| {
            commands
                .send(WorkerCommand::Explore(explore_session::Input::Submission(
                    Box::new(request),
                )))
                .map_err(|_| "The reviewer is closed".to_owned())
        },
        events,
    )
}

fn share_comments(
    comments: &comments::Worker,
    turns: Option<&vision_turns::TurnLog>,
    bus: Option<Result<PathBuf, String>>,
) {
    if let Some(turns) = turns {
        comments.log_turns(turns.clone());
    }
    if let Some(bus) = bus {
        comments.publish_on_bus(bus);
    }
}

fn thread_events(
    commands: &Sender<WorkerCommand>,
    messages: ApplicationEventSender,
    page: Arc<PageThreads>,
) -> impl Fn(comments::Event) + Send + 'static {
    let explore = inbox_for(commands.clone());
    move |event| {
        page.observe(&event);
        if let comments::Event::RoundMessage(message) = &event {
            explore.deliver(explore_session::Input::RoundMessage(message.clone()));
        }
        publish_thread_event(&messages, event);
    }
}

fn publish_thread_event(messages: &ApplicationEventSender, event: comments::Event) {
    match event {
        comments::Event::Loaded(event) => {
            let _ = messages.send(event);
        }
        comments::Event::Posted(event) => {
            let _ = messages.send(event);
        }
        comments::Event::Wakeup {
            review_unit,
            notice,
        } => {
            let _ = messages.send(ui_events::ThreadWakeupReported {
                review_unit,
                notice,
            });
        }
        comments::Event::RoundMessage(_) => {}
        comments::Event::Error(text) => {
            let _ = messages.send(ui_events::ToastRequested {
                text,
                kind: toasts::ToastKind::Error,
            });
        }
    }
}

struct Performer<'a, 't> {
    effects: &'a Effects,
    terminal: &'a mut TerminalExecutor<'t>,
}

impl ActionExecutors for Performer<'_, '_> {
    fn explore(&mut self, command: review_explore::Command) -> eyre::Result<()> {
        self.effects
            .commands
            .send(WorkerCommand::Explore(explore_session::Input::Command(
                command,
            )))?;
        Ok(())
    }

    fn open_link(&mut self, url: String) -> eyre::Result<()> {
        self.effects.open_link(url);
        Ok(())
    }

    fn explore_page(&mut self, action: ExplorePageAction) -> eyre::Result<()> {
        match action {
            ExplorePageAction::Open(screen) => self.effects.open_page(screen),
            ExplorePageAction::OpenTunnel => self.effects.share_round(true),
            ExplorePageAction::CloseTunnel => self.effects.share_round(false),
        }
        Ok(())
    }

    fn thread(&mut self, command: review_threads::ThreadCommand) -> eyre::Result<()> {
        self.effects
            .front()
            .comments
            .send(comments::Command::Thread(command));
        Ok(())
    }

    fn document(&mut self, action: DocumentAction) -> eyre::Result<()> {
        let effects = self.effects;
        match action {
            DocumentAction::Load(load) => effects.load_documents(load)?,
            DocumentAction::Highlight(request) => effects
                .front()
                .highlighting
                .submit(request)
                .map_err(eyre::Report::msg)?,
            DocumentAction::Search(request) => effects.front().search.submit(request),
            DocumentAction::WatchSource(path) => {
                if let Some(watches) = &effects.source_watches {
                    watches.watch(path.as_deref());
                }
            }
        }
        Ok(())
    }

    fn lsp(&mut self, action: LspAction) -> eyre::Result<()> {
        let effects = self.effects;
        match action {
            LspAction::OpenDocument(path) => effects.lsp.open_document(path),
            LspAction::Request {
                operation,
                mut query,
            } => {
                query.path = effects.resolve(query.path);
                effects.lsp.request(operation, query)
            }
            LspAction::Restart => effects.lsp.restart(),
        }
        .map_err(eyre::Report::msg)
    }

    fn settings(&mut self, action: SettingsAction) -> eyre::Result<()> {
        let settings = &self.effects.store;
        match action {
            SettingsAction::SaveFilePaneWidth(columns) => settings.save_file_pane_width(columns)?,
            SettingsAction::SaveEditorKeymap(keymap) => settings.save_editor_keymap(keymap)?,
            SettingsAction::SaveExplorePage(setting) => {
                let saved = settings.save_explore_page_setting(setting)?;
                if let Some(sharing) = &self.effects.page_sharing {
                    sharing.apply(&saved.network);
                }
                let _ = self
                    .effects
                    .messages
                    .send(ui_events::ExplorePageSettingsLoaded(saved));
            }
            SettingsAction::SaveExploreWritingStyle(writing) => {
                let saved = settings.save_explore_writing_style(writing)?;
                let _ = self
                    .effects
                    .messages
                    .send(ui_events::ExploreRoundSettingsLoaded(saved));
            }
            SettingsAction::SaveExploreRunAhead(run_ahead) => {
                let saved = settings.save_explore_run_ahead(run_ahead)?;
                let _ = self
                    .effects
                    .messages
                    .send(ui_events::ExploreRoundSettingsLoaded(saved));
            }
        }
        Ok(())
    }

    fn repository(&mut self, action: RepositoryAction) -> eyre::Result<()> {
        self.effects
            .commands
            .send(WorkerCommand::Repository(action))?;
        Ok(())
    }

    fn terminal(&mut self, action: TerminalAction) -> eyre::Result<ControlFlow<()>> {
        (self.terminal)(action)
    }
}

#[cfg(test)]
pub(super) mod fixture;

#[cfg(test)]
#[path = "effects.tests.rs"]
mod tests;
