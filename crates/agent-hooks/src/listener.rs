use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead, BufReader, Write as _};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use herdr_client::protocol::{Agent, AgentPort, PaneId};

use crate::directory::{HookDirectory, HookRoot, HookedSession};
use crate::wire::{AgentEvent, Answer, Report, SessionSource};

const READ_LIMIT: Duration = Duration::from_secs(1);

const CODEX: &str = "codex";

static REVIEWERS: AtomicU64 = AtomicU64::new(0);

pub struct AgentHooks {
    directory: HookDirectory,
    socket: PathBuf,
    expected: Arc<Expectations>,
    codex_sessions: Mutex<HashMap<PaneId, String>>,
    stopped: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HookedAgent {
    Pane {
        pane: PaneId,
        session: Option<String>,
    },
    Codex {
        session: String,
    },
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum Named {
    Pane(PaneId),
    Session(String),
}

impl HookedAgent {
    fn named(&self) -> Named {
        match self {
            Self::Pane { pane, .. } => Named::Pane(pane.clone()),
            Self::Codex { session } => Named::Session(session.clone()),
        }
    }
}

#[derive(Default)]
struct Expectations {
    agents: Mutex<HashMap<Named, Vec<Awaited>>>,
    heard: Condvar,
    next: AtomicU64,
}

struct Awaited {
    id: u64,
    awaiting: Awaiting,
    block: String,
    heard: Option<Heard>,
}

enum Awaiting {
    Resume {
        session: String,
    },
    Prompt {
        session: Option<String>,
        text: String,
        others: OtherPrompts,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OtherPrompts {
    // Herdr types a prompt at once, among the keys someone presses there.
    Block,
    // herdr-core's terminal.send types a prompt only once nobody typed for about 3 s, and holds
    // the keys pressed while it types.
    Pass,
}

impl OtherPrompts {
    pub fn for_host(port: &dyn AgentPort) -> Self {
        if port.types_prompts_whole() {
            Self::Pass
        } else {
            Self::Block
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Heard {
    Resumed,
    Blocked { prompt: String },
    Submitted,
    Joined { left: String },
    Stopped,
}

pub struct Expectation {
    expected: Arc<Expectations>,
    agent: Named,
    id: u64,
}

impl AgentHooks {
    pub fn listen(directory: &HookDirectory) -> io::Result<Self> {
        directory.create()?;
        directory.remove_stale_sockets();
        let number = REVIEWERS.fetch_add(1, Ordering::Relaxed);
        let socket = directory.socket(std::process::id(), number);
        let listener = crate::address::bind(&socket)?;
        let expected = Arc::new(Expectations::default());
        let stopped = Arc::new(AtomicBool::new(false));
        let thread = {
            let (expected, stopped) = (Arc::clone(&expected), Arc::clone(&stopped));
            thread::spawn(move || {
                for stream in listener.incoming() {
                    if stopped.load(Ordering::Acquire) {
                        return;
                    }
                    if let Ok(stream) = stream {
                        expected.take(&stream);
                    }
                }
            })
        };
        Ok(Self {
            directory: directory.clone(),
            socket,
            expected,
            codex_sessions: Mutex::default(),
            stopped,
            thread: Some(thread),
        })
    }

    pub fn name_session(&self, pane: &PaneId, session: &str) {
        if self.directory.root().is_codex_fork(session) {
            return;
        }
        self.codex_sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(pane.clone(), session.to_owned());
    }

    pub fn named_session(&self, pane: &PaneId) -> Option<String> {
        self.codex_sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(pane)
            .cloned()
    }

    pub fn root(&self) -> HookRoot {
        self.directory.root()
    }

    // Herdr may report another pane's session for a Codex pane.
    pub fn hooked(&self, port: &dyn AgentPort, agent: &Agent) -> Option<HookedAgent> {
        if agent.agent.as_deref() == Some(CODEX) {
            let session = self
                .codex_sessions
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get(&agent.pane_id)
                .cloned()?;
            return self
                .directory
                .root()
                .runs_codex_hook(&session)
                .then_some(HookedAgent::Codex { session });
        }
        let session = agent.agent_session.as_ref()?;
        let started: Vec<HookedSession> = self
            .directory
            .hooked_sessions(&agent.pane_id)
            .into_iter()
            .filter(|started| started.session == session.value && started.process.is_running())
            .collect();
        let running = !started.is_empty()
            && port
                .pane_process_info(&agent.pane_id)
                .is_ok_and(|processes| {
                    processes.foreground_processes.iter().any(|process| {
                        started
                            .iter()
                            .any(|started| started.process.pid == process.pid)
                    })
                });
        running.then(|| HookedAgent::Pane {
            pane: agent.pane_id.clone(),
            session: Some(session.value.clone()),
        })
    }

    pub fn expect_resume(&self, agent: &HookedAgent, session: &str, block: &str) -> Expectation {
        self.expect(
            agent,
            Awaiting::Resume {
                session: session.to_owned(),
            },
            block,
        )
    }

    pub fn expect_prompt(
        &self,
        agent: &HookedAgent,
        text: &str,
        block: &str,
        others: OtherPrompts,
    ) -> Expectation {
        let session = match agent {
            HookedAgent::Pane { session, .. } => session.clone(),
            HookedAgent::Codex { .. } => None,
        };
        self.expect(
            agent,
            Awaiting::Prompt {
                session,
                text: text.to_owned(),
                others,
            },
            block,
        )
    }

    fn expect(&self, agent: &HookedAgent, awaiting: Awaiting, block: &str) -> Expectation {
        let id = self.expected.next.fetch_add(1, Ordering::Relaxed);
        let agent = agent.named();
        self.expected
            .lock()
            .entry(agent.clone())
            .or_default()
            .push(Awaited {
                id,
                awaiting,
                block: block.to_owned(),
                heard: None,
            });
        Expectation {
            expected: Arc::clone(&self.expected),
            agent,
            id,
        }
    }
}

impl Drop for AgentHooks {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        // std's UnixListener blocks in accept with no way to cancel it.
        let woken = crate::address::connect(&self.socket).is_ok();
        let _ = fs::remove_file(&self.socket);
        if let Some(thread) = self.thread.take()
            && woken
        {
            let _ = thread.join();
        }
    }
}

impl Expectations {
    fn lock(&self) -> MutexGuard<'_, HashMap<Named, Vec<Awaited>>> {
        self.agents.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn take(&self, stream: &UnixStream) {
        let _ = stream.set_read_timeout(Some(READ_LIMIT));
        let mut line = String::new();
        if BufReader::new(stream).read_line(&mut line).is_err() {
            return;
        }
        let Ok(report) = serde_json::from_str::<Report>(&line) else {
            return;
        };
        match report.event {
            AgentEvent::SessionStarted {
                session,
                source: SessionSource::Resume,
            } => {
                if let Some(pane) = report.pane {
                    self.resumed(&Named::Pane(pane), &session);
                }
            }
            AgentEvent::SessionStarted { .. } => {}
            AgentEvent::PromptSubmitted { prompt, session } => {
                let agent = match (report.pane, &session) {
                    (Some(pane), _) => Some(Named::Pane(pane)),
                    (None, Some(session)) => Some(Named::Session(session.clone())),
                    (None, None) => None,
                };
                let block =
                    agent.and_then(|agent| self.submitted(&agent, &prompt, session.as_deref()));
                let Ok(mut answer) = serde_json::to_vec(&Answer { block }) else {
                    return;
                };
                answer.push(b'\n');
                let _ = (&mut &*stream).write_all(&answer);
            }
        }
    }

    fn resumed(&self, agent: &Named, session: &str) {
        let mut agents = self.lock();
        let mut heard = false;
        for awaited in agents.get_mut(agent).into_iter().flatten() {
            if matches!(&awaited.awaiting, Awaiting::Resume { session: expected } if expected == session)
                && awaited.heard.is_none()
            {
                awaited.heard = Some(Heard::Resumed);
                heard = true;
            }
        }
        if heard {
            self.heard.notify_all();
        }
    }

    fn submitted(&self, agent: &Named, prompt: &str, session: Option<&str>) -> Option<String> {
        let mut agents = self.lock();
        let armed = agents.get_mut(agent)?;
        let block = armed
            .iter()
            .find(|awaited| awaited.blocks(prompt, session))
            .map(|awaited| awaited.block.clone());
        let mut heard = false;
        for awaited in armed.iter_mut().filter(|awaited| awaited.heard.is_none()) {
            awaited.heard = awaited.hears(prompt, session, block.is_some());
            heard |= awaited.heard.is_some();
        }
        if heard {
            self.heard.notify_all();
        }
        block
    }
}

impl Awaited {
    fn blocks(&self, prompt: &str, session: Option<&str>) -> bool {
        match &self.awaiting {
            Awaiting::Resume { .. } => true,
            Awaiting::Prompt { text, others, .. } => {
                let mine = prompt.contains(text.trim());
                self.heard.is_none()
                    && (mine || *others == OtherPrompts::Block)
                    && (self.on_another_session(session) || prompt.trim() != text.trim())
            }
        }
    }

    fn on_another_session(&self, session: Option<&str>) -> bool {
        match &self.awaiting {
            Awaiting::Prompt {
                session: Some(expected),
                ..
            } => session.is_some_and(|session| session != expected),
            _ => false,
        }
    }

    fn hears(&self, prompt: &str, session: Option<&str>, blocked: bool) -> Option<Heard> {
        match &self.awaiting {
            Awaiting::Resume { .. } => blocked.then(|| Heard::Blocked {
                prompt: prompt.to_owned(),
            }),
            Awaiting::Prompt { text, .. } if prompt.contains(text.trim()) => Some(if !blocked {
                Heard::Submitted
            } else if self.on_another_session(session) || prompt.trim() == text.trim() {
                Heard::Stopped
            } else {
                Heard::Joined {
                    left: prompt.replacen(text.trim(), "", 1).trim().to_owned(),
                }
            }),
            Awaiting::Prompt { .. } => None,
        }
    }
}

impl Expectation {
    pub fn wait(&self, limit: Duration) -> Option<Heard> {
        let deadline = Instant::now() + limit;
        let mut agents = self.expected.lock();
        loop {
            let armed = agents
                .get(&self.agent)?
                .iter()
                .find(|awaited| awaited.id == self.id)?;
            if let Some(heard) = &armed.heard {
                return Some(heard.clone());
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return None;
            }
            agents = self
                .expected
                .heard
                .wait_timeout(agents, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }
}

impl Drop for Expectation {
    fn drop(&mut self) {
        let mut agents = self.expected.lock();
        if let Some(armed) = agents.get_mut(&self.agent) {
            armed.retain(|awaited| awaited.id != self.id);
            if armed.is_empty() {
                agents.remove(&self.agent);
            }
        }
    }
}

#[cfg(test)]
#[path = "listener.tests.rs"]
mod tests;
