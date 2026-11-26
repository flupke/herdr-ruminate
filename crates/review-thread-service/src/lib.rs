mod access;
mod agent_identity;
mod delivery;
mod notification;
mod pinned_agent;
mod state;
mod wakeup;

use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};

use herdr_client::protocol::{AgentPort, AgentTarget, HerdrEvent};
use review_mcp::Endpoint;
use review_store::ReviewStore;
use review_threads::{MessageId, ThreadCommand, WakeupNotice};
use review_types::ReviewUnit;

pub use delivery::{
    DispatchObserver, Drained, PromptCancellation, PromptError, PromptHold, PromptReceipt,
    PromptSender,
};
pub use pinned_agent::PinnedAgent;
use state::State;

#[derive(Debug)]
pub enum Command {
    Thread(ThreadCommand),
    ActiveAgentChanged,
    Observe(HerdrEvent),
}

enum Input {
    Ui(Command),
    Mcp(review_mcp::Request),
    Prompt(delivery::PromptRequest),
    #[cfg(any(test, feature = "flush"))]
    Flush(Sender<()>),
    Notification {
        token: String,
        through: u64,
        news: NotificationNews,
    },
    LogTurns(vision_turns::TurnLog),
    Withdrawn,
    Bus(BusInput),
    Stop,
}

enum NotificationNews {
    Waits(Option<herdr_client::protocol::PromptWait>),
    Outcome(Result<(), PromptError>),
}

enum BusInput {
    Open(Result<std::path::PathBuf, String>),
    Publish(review_words_bus::UserWords),
    Failed(String),
}

pub enum Event {
    Loaded(ui_events::ReviewThreadsLoaded),
    Posted(ui_events::ThreadPostFinished),
    RoundMessage(RoundMessage),
    Wakeup {
        review_unit: ReviewUnit,
        notice: Option<WakeupNotice>,
    },
    Error(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoundMessage {
    pub review_unit: ReviewUnit,
    pub round: String,
    pub message: MessageId,
}

pub struct Worker {
    sender: Sender<Input>,
    gate: std::sync::Arc<delivery::PromptGate>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for Worker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Worker").finish_non_exhaustive()
    }
}

impl Worker {
    pub fn prompt_sender(&self) -> PromptSender {
        PromptSender {
            sender: self.sender.clone(),
            gate: self.gate.clone(),
        }
    }

    pub fn start(
        store: ReviewStore,
        port: impl AgentPort + 'static,
        hooks: Option<std::sync::Arc<agent_hooks::AgentHooks>>,
        target: AgentTarget,
        endpoint: Result<Endpoint, String>,
        explore: impl Fn(review_mcp::Request) -> Result<(), String> + Send + Sync + 'static,
        publish: impl Fn(Event) + Send + 'static,
    ) -> Self {
        let (sender, receiver) = mpsc::channel();
        let requests = sender.clone();
        let inputs = sender.clone();
        let gate = std::sync::Arc::<delivery::PromptGate>::default();
        let courier_gate = gate.clone();
        let thread = thread::spawn(move || {
            let server = endpoint.and_then(|endpoint| {
                review_mcp::Server::start(endpoint, move |request| {
                    if request.operation.belongs_to_explore() {
                        return explore(request);
                    }
                    requests
                        .send(Input::Mcp(request))
                        .map_err(|_| "The reviewer is closed".into())
                })
            });
            let available = server.is_ok();
            if let Err(error) = &server {
                publish(Event::Error(error.clone()));
            }
            let port: std::sync::Arc<dyn AgentPort> = std::sync::Arc::new(port);
            let courier = delivery::Courier::start(port.clone(), courier_gate, hooks);
            State::new(
                store,
                port,
                target,
                available,
                Box::new(publish),
                inputs,
                courier,
            )
            .run(&receiver);
            drop(server);
        });
        Self {
            sender,
            gate,
            thread: Some(thread),
        }
    }

    pub fn send(&self, command: Command) {
        let _ = self.sender.send(Input::Ui(command));
    }

    pub fn log_turns(&self, turns: vision_turns::TurnLog) {
        let _ = self.sender.send(Input::LogTurns(turns));
    }

    pub fn publish_on_bus(&self, bus: Result<std::path::PathBuf, String>) {
        let _ = self.sender.send(Input::Bus(BusInput::Open(bus)));
    }

    pub fn thread_commands(&self) -> ThreadCommands {
        ThreadCommands(self.sender.clone())
    }
}

#[derive(Clone)]
pub struct ThreadCommands(Sender<Input>);

impl ThreadCommands {
    pub fn send(&self, command: ThreadCommand) {
        let _ = self.0.send(Input::Ui(Command::Thread(command)));
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.sender.send(Input::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
