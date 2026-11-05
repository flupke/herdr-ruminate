//! herdr-core's terminals, as its API gives them, and the agents and processes the reviewer
//! reads from them.

use std::collections::BTreeMap;
use std::path::PathBuf;

use herdr_client::protocol::{
    Agent, AgentSession, AgentStatus, PaneId, PaneProcess, PaneProcessInfo, TabId, WorkspaceId,
};
use serde::Deserialize;

/// The label herdr-app gives the terminals of a space: its ID.
const SPACE_LABEL: &str = "herdr.space";
/// The label herdr-app gives the terminals of a tab: its ID.
const TAB_LABEL: &str = "herdr.tab";

/// The label of a terminal that names the session its agent runs, as `<agent>:<session>`: the
/// agent's hooks set it, since herdr-core keeps no agent session (ADR 0016).
pub(crate) const SESSION_LABEL: &str = "herdr-reviewer.session";

/// The value of [`SESSION_LABEL`] for the session `session` of the agent `agent`.
pub(crate) fn session_label(agent: &str, session: &str) -> String {
    format!("{agent}:{session}")
}

/// The session that the [`SESSION_LABEL`] among `labels` names for the agent `agent`: none when
/// it names another agent's, which a program the terminal ran before left.
fn session_of(labels: &BTreeMap<String, String>, agent: &str) -> Option<AgentSession> {
    session_named(labels.get(SESSION_LABEL)?, agent)
}

/// The session of the agent `agent` that the [`SESSION_LABEL`] value `label` names.
fn session_named(label: &str, agent: &str) -> Option<AgentSession> {
    let (named, session) = label.split_once(':')?;
    (named == agent && !session.is_empty()).then(|| AgentSession {
        source: SESSION_LABEL.to_owned(),
        agent: agent.to_owned(),
        kind: "session_id".to_owned(),
        value: session.to_owned(),
    })
}

/// One terminal of herdr-core, live or finished (`Terminal` in `hc schema`).
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Terminal {
    /// `t7`: the terminal, for good, on its core.
    pub(crate) address: String,
    #[serde(default)]
    pub(crate) labels: BTreeMap<String, String>,
    #[serde(default)]
    name: Option<String>,
    /// The folder it started in.
    #[serde(default)]
    cwd: Option<PathBuf>,
    /// What its program last reported with OSC 7501; `None` when it never did, or once it
    /// exited.
    #[serde(default)]
    pub(crate) status: Option<Status>,
    /// The leader of its foreground process group; `None` once it finished.
    #[serde(default)]
    pub(crate) foreground: Option<Foreground>,
    /// `live` or `finished`.
    #[serde(default)]
    state: String,
}

/// A program's OSC 7501 report: its own words, which herdr-core does not translate.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub(crate) struct Status {
    /// `idle`, `working`, `done`, `blocked` or `error`.
    state: String,
    /// The program that reports, such as `claude-code` or `codex`.
    #[serde(default)]
    app: Option<String>,
}

/// The leader of a terminal's foreground process group.
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Foreground {
    pid: u32,
    name: String,
    argv: Vec<String>,
    /// Its working directory now.
    #[serde(default)]
    cwd: Option<PathBuf>,
}

/// Claude Code's program, and the agent's name in Herdr's words.
const CLAUDE: &str = "claude";
/// The `app` Claude Code reports with OSC 7501.
const CLAUDE_CODE_APP: &str = "claude-code";

/// The status of the agent a terminal runs: what its program reported last, or an unknown one
/// for Claude Code in the foreground that reports none, once its terminal's session label
/// (`label`) names a session of Claude Code's, which the reviewer's hook sets as a session
/// starts and removes as it ends. Claude Code 2.1.318 can move its conversation into a
/// background session, on a pty of its daemon's, while the `claude` in the terminal stays in
/// its foreground: the session's status reaches no terminal, but its hook still labels this one.
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
    /// Whether the leader runs `program`, by its name or its first argument's file name.
    fn runs(&self, program: &str) -> bool {
        self.name == program
            || self
                .argv
                .first()
                .and_then(|first| std::path::Path::new(first).file_name())
                .is_some_and(|name| name == program)
    }
}

/// The space and the tab of herdr-app that a terminal with `labels` shows in.
pub(crate) fn place_of(labels: &BTreeMap<String, String>) -> Option<(WorkspaceId, TabId)> {
    let space = labels.get(SPACE_LABEL).cloned().map(WorkspaceId)?;
    let tab = labels.get(TAB_LABEL).cloned().map(TabId)?;
    Some((space, tab))
}

impl Status {
    /// The status in Herdr's words. `error`, which Herdr lacks, is unknown: the agent neither
    /// waits for a prompt nor works on one.
    pub(crate) fn agent_status(&self) -> AgentStatus {
        match self.state.as_str() {
            "idle" => AgentStatus::Idle,
            "working" => AgentStatus::Working,
            "blocked" => AgentStatus::Blocked,
            "done" => AgentStatus::Done,
            _ => AgentStatus::Unknown,
        }
    }

    /// The agent's name as Herdr gives it, which the reviewer matches to pick an agent's forks
    /// and hooks: herdr-core reports Claude Code as `claude-code`, Herdr as `claude`.
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

    /// The space and the tab of herdr-app the terminal shows in, by its labels.
    pub(crate) fn place(&self) -> Option<(WorkspaceId, TabId)> {
        place_of(&self.labels)
    }

    /// Whether its program still runs: not once it finished.
    pub(crate) fn is_live(&self) -> bool {
        self.state == "live"
    }

    /// The agent the terminal runs: a program that reports its status, or Claude Code in the
    /// foreground on a session its [`SESSION_LABEL`] names, in a tab of herdr-app, on the
    /// session that label names, if it names one.
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

    /// The terminal's foreground process group, which herdr-core names by its leader alone.
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
