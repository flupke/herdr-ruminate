use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::Result;
use crate::client::EventCanceller;

pub mod method {
    pub const SESSION_SNAPSHOT: &str = "session.snapshot";
    pub const AGENT_LIST: &str = "agent.list";
    pub const AGENT_GET: &str = "agent.get";
    pub const AGENT_READ: &str = "agent.read";
    pub const AGENT_FOCUS: &str = "agent.focus";
    pub const AGENT_PROMPT: &str = "agent.prompt";
    pub const PANE_GET: &str = "pane.get";
    pub const PANE_LAYOUT: &str = "pane.layout";
    pub const PLUGIN_PANE_OPEN: &str = "plugin.pane.open";
    pub const PLUGIN_PANE_FOCUS: &str = "plugin.pane.focus";
    pub const PLUGIN_PANE_CLOSE: &str = "plugin.pane.close";
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct PaneId(pub String);

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct TabId(pub String);

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct WorkspaceId(pub String);

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct EntrypointId(pub String);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PanePlacement {
    Split,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitDirection {
    Right,
    Down,
}

impl SplitDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Right => "right",
            Self::Down => "down",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub struct PaneSize {
    pub width: u16,
    pub height: u16,
}

impl PaneSize {
    // A terminal cell is a little over twice as tall as it is wide.
    pub fn split_direction(self) -> SplitDirection {
        if 2 * u32::from(self.width) >= 5 * u32::from(self.height) {
            SplitDirection::Right
        } else {
            SplitDirection::Down
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PromptWait {
    TypedText,
    NoInputLine,
    UnreadAgent(String),
}

impl std::fmt::Display for PromptWait {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TypedText => formatter.write_str(
                "the agent's input line shows text someone typed; it goes once that text is sent \
                 or cleared",
            ),
            Self::NoInputLine => formatter.write_str(
                "the agent's screen shows no input line, as while a dialog shows; it goes once \
                 the input line is back and empty",
            ),
            Self::UnreadAgent(agent) => write!(
                formatter,
                "the reviewer cannot read the input line of {agent}, so it types nothing there"
            ),
        }
    }
}

pub trait AgentPort: Send + Sync {
    fn session_snapshot(&self) -> Result<SessionSnapshot>;

    fn list_agents(&self) -> Result<Vec<Agent>>;

    fn get_agent(&self, pane_id: &PaneId) -> Result<Option<Agent>>;

    fn pane_process_info(&self, pane_id: &PaneId) -> Result<PaneProcessInfo>;

    fn prompt_agent(&self, pane_id: &PaneId, text: &str) -> Result<()>;

    fn submit_prompt(&self, pane_id: &PaneId, text: &str) -> Result<()>;

    fn prompt_agent_telling_waits(
        &self,
        pane_id: &PaneId,
        text: &str,
        waits: &mut dyn FnMut(Option<PromptWait>),
        withdrawal: &EventCanceller,
    ) -> Result<()> {
        let _ = waits;
        if withdrawal.is_cancelled() {
            return Err(crate::Error::Cancelled);
        }
        self.prompt_agent(pane_id, text)
    }

    fn submit_prompt_telling_waits(
        &self,
        pane_id: &PaneId,
        text: &str,
        waits: &mut dyn FnMut(Option<PromptWait>),
        withdrawal: &EventCanceller,
    ) -> Result<()> {
        let _ = waits;
        if withdrawal.is_cancelled() {
            return Err(crate::Error::Cancelled);
        }
        self.submit_prompt(pane_id, text)
    }

    // herdr-core's events tell each change of an agent's session; Herdr's do not.
    fn announces_sessions(&self) -> bool {
        false
    }

    // herdr-core's `terminal.send` types a prompt whole, once nobody typed for a while, holding
    // the keys pressed meanwhile; Herdr types at once, among the keys someone presses.
    fn types_prompts_whole(&self) -> bool {
        false
    }
}

impl<T: AgentPort + ?Sized> AgentPort for Arc<T> {
    fn session_snapshot(&self) -> Result<SessionSnapshot> {
        (**self).session_snapshot()
    }

    fn list_agents(&self) -> Result<Vec<Agent>> {
        (**self).list_agents()
    }

    fn get_agent(&self, pane_id: &PaneId) -> Result<Option<Agent>> {
        (**self).get_agent(pane_id)
    }

    fn pane_process_info(&self, pane_id: &PaneId) -> Result<PaneProcessInfo> {
        (**self).pane_process_info(pane_id)
    }

    fn prompt_agent(&self, pane_id: &PaneId, text: &str) -> Result<()> {
        (**self).prompt_agent(pane_id, text)
    }

    fn submit_prompt(&self, pane_id: &PaneId, text: &str) -> Result<()> {
        (**self).submit_prompt(pane_id, text)
    }

    fn prompt_agent_telling_waits(
        &self,
        pane_id: &PaneId,
        text: &str,
        waits: &mut dyn FnMut(Option<PromptWait>),
        withdrawal: &EventCanceller,
    ) -> Result<()> {
        (**self).prompt_agent_telling_waits(pane_id, text, waits, withdrawal)
    }

    fn submit_prompt_telling_waits(
        &self,
        pane_id: &PaneId,
        text: &str,
        waits: &mut dyn FnMut(Option<PromptWait>),
        withdrawal: &EventCanceller,
    ) -> Result<()> {
        (**self).submit_prompt_telling_waits(pane_id, text, waits, withdrawal)
    }

    fn announces_sessions(&self) -> bool {
        (**self).announces_sessions()
    }

    fn types_prompts_whole(&self) -> bool {
        (**self).types_prompts_whole()
    }
}

pub trait AgentHost: AgentPort + std::fmt::Debug {
    fn forward_events(
        &self,
        canceller: &EventCanceller,
        send: &mut dyn FnMut(HerdrEvent) -> bool,
    ) -> Result<()>;

    fn forward_agent_status(
        &self,
        pane_id: &PaneId,
        canceller: &EventCanceller,
        send: &mut dyn FnMut(AgentStatus) -> bool,
    ) -> Result<()>;

    fn report_agent_session(&self, pane_id: &PaneId, report: &SessionReport<'_>) -> Result<()>;
}

pub trait HerdrReader: Send + Sync {
    fn read_agent_screen(&self, pane_id: &PaneId) -> Result<String>;

    fn list_plugin_panes(&self, tab_id: &TabId) -> Result<Vec<PluginPane>>;

    fn pane_size(&self, pane_id: &PaneId) -> Result<PaneSize>;
}

pub trait HerdrWriter: Send + Sync {
    fn open_plugin_pane(&self, request: &OpenPluginPane) -> Result<PluginPane>;

    fn focus_plugin_pane(&self, pane_id: &PaneId) -> Result<()>;

    fn focus_agent(&self, pane_id: &PaneId) -> Result<()>;

    fn close_plugin_pane(&self, pane_id: &PaneId) -> Result<()>;
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct PluginContext {
    #[serde(default)]
    pub workspace_id: Option<WorkspaceId>,
    #[serde(default)]
    pub tab_id: Option<TabId>,
    #[serde(default)]
    pub focused_pane_id: Option<PaneId>,
    #[serde(default)]
    pub focused_pane_cwd: Option<PathBuf>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
pub struct SessionSnapshot {
    #[serde(default)]
    pub focused_pane_id: Option<PaneId>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Agent {
    pub pane_id: PaneId,
    pub tab_id: TabId,
    pub workspace_id: WorkspaceId,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub display_agent: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    pub agent_status: AgentStatus,
    #[serde(default)]
    pub agent_session: Option<AgentSession>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Idle,
    Working,
    Blocked,
    Done,
    #[default]
    Unknown,
}

impl AgentStatus {
    pub fn waits_for_prompt(self) -> bool {
        matches!(self, Self::Idle | Self::Done)
    }

    pub fn started_on_prompt(self) -> bool {
        matches!(self, Self::Working | Self::Blocked)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SessionReport<'a> {
    pub source: &'a str,
    pub agent: &'a str,
    pub seq: u64,
    pub session: &'a str,
    pub start: &'a str,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentSession {
    pub source: String,
    pub agent: String,
    pub kind: String,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PaneProcessInfo {
    pub pane_id: PaneId,
    #[serde(default)]
    pub foreground_process_group_id: Option<u32>,
    #[serde(default)]
    pub foreground_processes: Vec<PaneProcess>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PaneProcess {
    pub pid: u32,
    pub name: String,
    pub argv: Option<Vec<String>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HerdrEvent {
    PaneFocused(PaneId),
    AgentDetected {
        pane_id: PaneId,
        workspace_id: WorkspaceId,
        agent: Option<String>,
        released: bool,
        final_status: Option<AgentStatus>,
    },
    AgentSessionChanged(PaneId),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PluginPane {
    pub pane_id: PaneId,
    pub tab_id: TabId,
    pub workspace_id: WorkspaceId,
    pub entrypoint_id: EntrypointId,
}

#[derive(Clone, Debug, Serialize, Eq, PartialEq)]
pub struct OpenPluginPane {
    pub entrypoint: EntrypointId,
    pub placement: PanePlacement,
    pub target_pane_id: PaneId,
    pub direction: Option<SplitDirection>,
    pub cwd: PathBuf,
    pub focus: bool,
}

#[derive(Clone, Debug)]
pub struct AgentTarget {
    tab_id: TabId,
    focused_pane_ids: Arc<Mutex<Vec<PaneId>>>,
}

impl AgentTarget {
    pub fn new(tab_id: TabId, focused_pane_id: Option<PaneId>) -> Self {
        Self {
            tab_id,
            focused_pane_ids: Arc::new(Mutex::new(focused_pane_id.into_iter().collect())),
        }
    }

    pub fn tab_id(&self) -> &TabId {
        &self.tab_id
    }

    fn initialize(&mut self, port: &(impl AgentPort + ?Sized)) -> Result<()> {
        if let Some(pane_id) = port.session_snapshot()?.focused_pane_id {
            self.observe_focus(&pane_id);
        }
        Ok(())
    }

    /// # Panics
    /// Panics if another thread poisoned the shared focus lock.
    pub fn observe_focus(&mut self, pane_id: &PaneId) {
        let mut focused = self.focused_pane_ids.lock().expect("agent focus lock");
        focused.retain(|previous| previous != pane_id);
        focused.push(pane_id.clone());
    }

    pub fn resolve(&mut self, port: &(impl AgentPort + ?Sized)) -> Result<Option<Agent>> {
        let agents = port.list_agents()?;
        self.initialize(port)?;
        if let Some(agent) = self.current_agent(&agents) {
            return Ok(Some(agent));
        }

        let mut same_tab_agents = agents
            .into_iter()
            .filter(|agent| agent.tab_id == self.tab_id);
        let only_agent = same_tab_agents.next();
        if same_tab_agents.next().is_some() {
            return Ok(None);
        }
        Ok(only_agent)
    }

    fn current_agent(&self, agents: &[Agent]) -> Option<Agent> {
        self.focused_pane_ids
            .lock()
            .expect("agent focus lock")
            .iter()
            .rev()
            .find_map(|pane_id| {
                agents
                    .iter()
                    .find(|agent| agent.pane_id == *pane_id && agent.tab_id == self.tab_id)
                    .cloned()
            })
    }
}

#[cfg(test)]
#[path = "protocol.tests.rs"]
mod tests;
