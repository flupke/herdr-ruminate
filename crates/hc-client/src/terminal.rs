use std::collections::BTreeMap;
use std::path::PathBuf;

use herdr_client::protocol::{
    Agent, AgentSession, AgentStatus, PaneId, PaneProcess, PaneProcessInfo, TabId, WorkspaceId,
};
use serde::Deserialize;

const SPACE_LABEL: &str = "herdr.space";
const TAB_LABEL: &str = "herdr.tab";

// herdr-core keeps no agent session: the agent's hooks set this label.
pub(crate) const SESSION_LABEL: &str = "herdr-reviewer.session";

pub(crate) fn session_label(agent: &str, session: &str) -> String {
    format!("{agent}:{session}")
}

fn session_of(labels: &BTreeMap<String, String>, agent: &str) -> Option<AgentSession> {
    session_named(labels.get(SESSION_LABEL)?, agent)
}

fn session_named(label: &str, agent: &str) -> Option<AgentSession> {
    let (named, session) = label.split_once(':')?;
    (named == agent && !session.is_empty()).then(|| AgentSession {
        source: SESSION_LABEL.to_owned(),
        agent: agent.to_owned(),
        kind: "session_id".to_owned(),
        value: session.to_owned(),
    })
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Terminal {
    pub(crate) address: String,
    #[serde(default)]
    pub(crate) labels: BTreeMap<String, String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    cwd: Option<PathBuf>,
    #[serde(default)]
    pub(crate) status: Option<Status>,
    #[serde(default)]
    pub(crate) foreground: Option<Foreground>,
    #[serde(default)]
    state: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub(crate) struct Status {
    state: String,
    #[serde(default)]
    app: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Foreground {
    pid: u32,
    name: String,
    argv: Vec<String>,
    #[serde(default)]
    cwd: Option<PathBuf>,
}

const CLAUDE: &str = "claude";
const CLAUDE_CODE_APP: &str = "claude-code";

// Claude Code 2.1.318 can move its conversation to a background session on its daemon's pty: the
// `claude` left in the terminal's foreground reports no OSC 7501 status, but its hooks still run.
pub(crate) fn running(
    status: Option<&Status>,
    foreground: Option<&Foreground>,
    label: Option<&str>,
) -> Option<Status> {
    status.cloned().or_else(|| {
        foreground
            .filter(|leader| leader.runs(CLAUDE))
            .and(label.and_then(|label| session_named(label, CLAUDE)))
            .map(|_| Status {
                state: String::new(),
                app: Some(CLAUDE_CODE_APP.to_owned()),
            })
    })
}

impl Foreground {
    fn runs(&self, program: &str) -> bool {
        self.name == program
            || self
                .argv
                .first()
                .and_then(|first| std::path::Path::new(first).file_name())
                .is_some_and(|name| name == program)
    }
}

pub(crate) fn place_of(labels: &BTreeMap<String, String>) -> Option<(WorkspaceId, TabId)> {
    let space = labels.get(SPACE_LABEL).cloned().map(WorkspaceId)?;
    let tab = labels.get(TAB_LABEL).cloned().map(TabId)?;
    Some((space, tab))
}

impl Status {
    // Herdr has no status for OSC 7501's `error`.
    pub(crate) fn agent_status(&self) -> AgentStatus {
        match self.state.as_str() {
            "idle" => AgentStatus::Idle,
            "working" => AgentStatus::Working,
            "blocked" => AgentStatus::Blocked,
            "done" => AgentStatus::Done,
            _ => AgentStatus::Unknown,
        }
    }

    // herdr-core reports Claude Code as `claude-code`, Herdr as `claude`.
    pub(crate) fn agent(&self) -> Option<String> {
        self.app.as_deref().map(|app| match app {
            CLAUDE_CODE_APP => CLAUDE.to_owned(),
            other => other.to_owned(),
        })
    }
}

impl Terminal {
    fn pane_id(&self) -> PaneId {
        PaneId(self.address.clone())
    }

    pub(crate) fn place(&self) -> Option<(WorkspaceId, TabId)> {
        place_of(&self.labels)
    }

    pub(crate) fn is_live(&self) -> bool {
        self.state == "live"
    }

    pub(crate) fn agent(&self) -> Option<Agent> {
        let status = &running(
            self.status.as_ref(),
            self.foreground.as_ref(),
            self.labels.get(SESSION_LABEL).map(String::as_str),
        )?;
        let (workspace_id, tab_id) = self.place()?;
        let agent = status.agent();
        Some(Agent {
            pane_id: self.pane_id(),
            tab_id,
            workspace_id,
            name: self.name.clone(),
            display_agent: status.app.clone(),
            agent_session: agent
                .as_deref()
                .and_then(|agent| session_of(&self.labels, agent)),
            agent,
            agent_status: status.agent_status(),
            cwd: self
                .foreground
                .as_ref()
                .and_then(|foreground| foreground.cwd.clone())
                .or_else(|| self.cwd.clone()),
        })
    }

    // herdr-core gives only the leader of a terminal's foreground process group.
    pub(crate) fn process_info(&self) -> PaneProcessInfo {
        PaneProcessInfo {
            pane_id: self.pane_id(),
            foreground_process_group_id: self.foreground.as_ref().map(|leader| leader.pid),
            foreground_processes: self
                .foreground
                .iter()
                .map(|leader| PaneProcess {
                    pid: leader.pid,
                    name: leader.name.clone(),
                    argv: Some(leader.argv.clone()),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
#[path = "terminal.tests.rs"]
mod tests;
