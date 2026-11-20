//! An in-memory agent host for tests that exercise agent delivery without Herdr.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::protocol::{
    Agent, AgentPort, PaneId, PaneProcess, PaneProcessInfo, PromptWait, SessionSnapshot,
};
use crate::{Error, Result};

/// A prompt submitted to one agent pane.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SentPrompt {
    pub pane_id: PaneId,
    pub text: String,
}

/// Live agents, pane process groups and submitted prompts kept in memory.
///
/// Clones share the same host, so a test can keep one handle while the code under
/// test owns another.
#[derive(Clone, Debug, Default)]
pub struct InMemoryAgents(Arc<Mutex<Host>>);

#[derive(Debug, Default)]
struct Host {
    session: SessionSnapshot,
    agents: Vec<Agent>,
    process_groups: HashMap<PaneId, u32>,
    /// The command line of the foreground process of each pane that has one.
    commands: HashMap<PaneId, Vec<String>>,
    prompts: Vec<SentPrompt>,
    /// Panes whose agent reads each prompt without starting on it.
    swallowing: HashSet<PaneId>,
    /// Whether the host claims, as herdr-core does, that its events tell each change of an
    /// agent's session.
    announces_sessions: bool,
    /// Whether the host claims, as herdr-core does, that it types each prompt whole.
    types_prompts_whole: bool,
    /// Why the host refuses each prompt before it types it, if it does.
    refusing: Option<String>,
    /// Why the host holds each prompt to a pane before it types it, for a while.
    holding: HashMap<PaneId, PromptWait>,
}

impl InMemoryAgents {
    fn host(&self) -> MutexGuard<'_, Host> {
        self.0.lock().expect("in-memory agent host lock")
    }

    /// Replace the session snapshot that target resolution reads.
    pub fn set_session(&self, session: SessionSnapshot) {
        self.host().session = session;
    }

    /// Add a live agent, or replace the agent already running in its pane.
    pub fn upsert_agent(&self, agent: Agent) {
        let mut host = self.host();
        match host
            .agents
            .iter_mut()
            .find(|existing| existing.pane_id == agent.pane_id)
        {
            Some(existing) => *existing = agent,
            None => host.agents.push(agent),
        }
    }

    /// Remove the agent running in `pane_id`.
    pub fn remove_agent(&self, pane_id: &PaneId) {
        self.host().agents.retain(|agent| agent.pane_id != *pane_id);
    }

    /// Set the foreground process group Herdr reports for `pane_id`.
    pub fn set_process_group(&self, pane_id: &PaneId, group: u32) {
        self.host().process_groups.insert(pane_id.clone(), group);
    }

    /// Set the command line of the foreground process Herdr reports for `pane_id`.
    pub fn set_command(&self, pane_id: &PaneId, argv: &[&str]) {
        let argv = argv.iter().map(|argument| (*argument).to_owned()).collect();
        self.host().commands.insert(pane_id.clone(), argv);
    }

    /// Make the agent in `pane_id` read each prompt without starting on it, as an agent that
    /// drops a paste does, or start on each prompt again.
    pub fn swallow_prompts(&self, pane_id: &PaneId, swallow: bool) {
        let mut host = self.host();
        if swallow {
            host.swallowing.insert(pane_id.clone());
        } else {
            host.swallowing.remove(pane_id);
        }
    }

    /// Make the host claim, as herdr-core does, that its events tell each change of an agent's
    /// session: a worker stops asking again for a session.
    pub fn announce_sessions(&self) {
        self.host().announces_sessions = true;
    }

    /// Make the host claim, as herdr-core does, that it types each prompt whole: the hooks let
    /// a prompt without the reviewer's text go on while the reviewer waits for its own.
    pub fn type_prompts_whole(&self) {
        self.host().types_prompts_whole = true;
    }

    /// Make the host hold each prompt to `pane_id` for `wait` before it types it, as herdr-core's
    /// client does while the agent's input line shows text: it tells the wait, then that it
    /// ended, then types the prompt.
    pub fn hold_prompts(&self, pane_id: &PaneId, wait: PromptWait) {
        self.host().holding.insert(pane_id.clone(), wait);
    }

    /// Tells `waits` why the host holds the prompt to `pane_id`, then that it no longer does.
    fn hold(&self, pane_id: &PaneId, waits: &mut dyn FnMut(Option<PromptWait>)) {
        let wait = self.host().holding.get(pane_id).cloned();
        if let Some(wait) = wait {
            waits(Some(wait));
            waits(None);
        }
    }

    /// Make the host refuse each prompt before it types it, for `reason`, as herdr-core
    /// refuses one to an agent that waits on its user.
    pub fn refuse_prompts(&self, reason: &str) {
        self.host().refusing = Some(reason.to_owned());
    }

    /// Every prompt submitted so far, in submission order.
    pub fn prompts(&self) -> Vec<SentPrompt> {
        self.host().prompts.clone()
    }
}

impl AgentPort for InMemoryAgents {
    fn session_snapshot(&self) -> Result<SessionSnapshot> {
        Ok(self.host().session.clone())
    }

    fn list_agents(&self) -> Result<Vec<Agent>> {
        Ok(self.host().agents.clone())
    }

    fn get_agent(&self, pane_id: &PaneId) -> Result<Option<Agent>> {
        Ok(self
            .host()
            .agents
            .iter()
            .find(|agent| agent.pane_id == *pane_id)
            .cloned())
    }

    fn pane_process_info(&self, pane_id: &PaneId) -> Result<PaneProcessInfo> {
        let host = self.host();
        Ok(PaneProcessInfo {
            pane_id: pane_id.clone(),
            foreground_process_group_id: host.process_groups.get(pane_id).copied(),
            foreground_processes: host
                .commands
                .get(pane_id)
                .map(|argv| PaneProcess {
                    pid: 0,
                    name: argv.first().cloned().unwrap_or_default(),
                    argv: Some(argv.clone()),
                })
                .into_iter()
                .collect(),
        })
    }

    fn submit_prompt(&self, pane_id: &PaneId, text: &str) -> Result<()> {
        let mut host = self.host();
        if let Some(reason) = &host.refusing {
            return Err(Error::NotTyped {
                reason: reason.clone(),
            });
        }
        host.prompts.push(SentPrompt {
            pane_id: pane_id.clone(),
            text: text.to_owned(),
        });
        Ok(())
    }

    fn prompt_agent(&self, pane_id: &PaneId, text: &str) -> Result<()> {
        let mut host = self.host();
        if let Some(reason) = &host.refusing {
            return Err(Error::NotTyped {
                reason: reason.clone(),
            });
        }
        host.prompts.push(SentPrompt {
            pane_id: pane_id.clone(),
            text: text.to_owned(),
        });
        if host.swallowing.contains(pane_id) {
            return Err(Error::AgentNotStarted {
                message: "the agent showed no activity".into(),
            });
        }
        Ok(())
    }

    fn prompt_agent_telling_waits(
        &self,
        pane_id: &PaneId,
        text: &str,
        waits: &mut dyn FnMut(Option<PromptWait>),
    ) -> Result<()> {
        self.hold(pane_id, waits);
        self.prompt_agent(pane_id, text)
    }

    fn submit_prompt_telling_waits(
        &self,
        pane_id: &PaneId,
        text: &str,
        waits: &mut dyn FnMut(Option<PromptWait>),
    ) -> Result<()> {
        self.hold(pane_id, waits);
        self.submit_prompt(pane_id, text)
    }

    fn announces_sessions(&self) -> bool {
        self.host().announces_sessions
    }

    fn types_prompts_whole(&self) -> bool {
        self.host().types_prompts_whole
    }
}
