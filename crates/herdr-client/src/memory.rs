use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use crate::client::EventCanceller;
use crate::protocol::{
    Agent, AgentPort, PaneId, PaneProcess, PaneProcessInfo, PromptWait, SessionSnapshot,
};
use crate::{Error, Result};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SentPrompt {
    pub pane_id: PaneId,
    pub text: String,
}

#[derive(Clone, Debug, Default)]
pub struct InMemoryAgents(Arc<Shared>);

#[derive(Debug, Default)]
struct Shared {
    host: Mutex<Host>,
    changed: Condvar,
}

const WITHDRAWAL_LOOK: Duration = Duration::from_millis(5);

#[derive(Debug, Default)]
struct Host {
    session: SessionSnapshot,
    agents: Vec<Agent>,
    process_groups: HashMap<PaneId, u32>,
    commands: HashMap<PaneId, Vec<String>>,
    prompts: Vec<SentPrompt>,
    swallowing: HashSet<PaneId>,
    announces_sessions: bool,
    types_prompts_whole: bool,
    refusing: Option<String>,
    holding: HashMap<PaneId, PromptWait>,
    typed_lines: HashSet<PaneId>,
    waiting: HashMap<PaneId, usize>,
}

impl InMemoryAgents {
    fn host(&self) -> MutexGuard<'_, Host> {
        self.0.host.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn set_session(&self, session: SessionSnapshot) {
        self.host().session = session;
    }

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

    pub fn remove_agent(&self, pane_id: &PaneId) {
        self.host().agents.retain(|agent| agent.pane_id != *pane_id);
    }

    pub fn set_process_group(&self, pane_id: &PaneId, group: u32) {
        self.host().process_groups.insert(pane_id.clone(), group);
    }

    pub fn set_command(&self, pane_id: &PaneId, argv: &[&str]) {
        let argv = argv.iter().map(|argument| (*argument).to_owned()).collect();
        self.host().commands.insert(pane_id.clone(), argv);
    }

    pub fn swallow_prompts(&self, pane_id: &PaneId, swallow: bool) {
        let mut host = self.host();
        if swallow {
            host.swallowing.insert(pane_id.clone());
        } else {
            host.swallowing.remove(pane_id);
        }
    }

    pub fn announce_sessions(&self) {
        self.host().announces_sessions = true;
    }

    pub fn type_prompts_whole(&self) {
        self.host().types_prompts_whole = true;
    }

    pub fn hold_prompts(&self, pane_id: &PaneId, wait: PromptWait) {
        self.host().holding.insert(pane_id.clone(), wait);
    }

    fn hold(&self, pane_id: &PaneId, waits: &mut dyn FnMut(Option<PromptWait>)) {
        let wait = self.host().holding.get(pane_id).cloned();
        if let Some(wait) = wait {
            waits(Some(wait));
            waits(None);
        }
    }

    pub fn type_in_line(&self, pane_id: &PaneId) {
        self.host().typed_lines.insert(pane_id.clone());
    }

    pub fn empty_line(&self, pane_id: &PaneId) {
        self.host().typed_lines.remove(pane_id);
        self.0.changed.notify_all();
    }

    pub fn wait_until_waiting(&self, pane_id: &PaneId, count: usize) {
        drop(
            self.0
                .changed
                .wait_while(self.host(), |host| {
                    host.waiting.get(pane_id).copied().unwrap_or_default() != count
                })
                .unwrap_or_else(PoisonError::into_inner),
        );
    }

    fn wait_for_line(
        &self,
        pane_id: &PaneId,
        waits: &mut dyn FnMut(Option<PromptWait>),
        withdrawal: &EventCanceller,
    ) -> Result<()> {
        self.hold(pane_id, waits);
        if withdrawal.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if !self.host().typed_lines.contains(pane_id) {
            return Ok(());
        }
        waits(Some(PromptWait::TypedText));
        let mut host = self.host();
        *host.waiting.entry(pane_id.clone()).or_default() += 1;
        self.0.changed.notify_all();
        while host.typed_lines.contains(pane_id) && !withdrawal.is_cancelled() {
            host = self
                .0
                .changed
                .wait_timeout(host, WITHDRAWAL_LOOK)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        let withdrawn = withdrawal.is_cancelled();
        if let Some(waiting) = host.waiting.get_mut(pane_id) {
            *waiting -= 1;
        }
        drop(host);
        self.0.changed.notify_all();
        if withdrawn {
            return Err(Error::Cancelled);
        }
        waits(None);
        Ok(())
    }

    pub fn refuse_prompts(&self, reason: &str) {
        self.host().refusing = Some(reason.to_owned());
    }

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
        withdrawal: &EventCanceller,
    ) -> Result<()> {
        self.wait_for_line(pane_id, waits, withdrawal)?;
        self.prompt_agent(pane_id, text)
    }

    fn submit_prompt_telling_waits(
        &self,
        pane_id: &PaneId,
        text: &str,
        waits: &mut dyn FnMut(Option<PromptWait>),
        withdrawal: &EventCanceller,
    ) -> Result<()> {
        self.wait_for_line(pane_id, waits, withdrawal)?;
        self.submit_prompt(pane_id, text)
    }

    fn announces_sessions(&self) -> bool {
        self.host().announces_sessions
    }

    fn types_prompts_whole(&self) -> bool {
        self.host().types_prompts_whole
    }
}
