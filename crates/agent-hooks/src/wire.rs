use herdr_client::protocol::PaneId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct Report {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane: Option<PaneId>,
    pub event: AgentEvent,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum AgentEvent {
    SessionStarted {
        session: String,
        source: SessionSource,
    },
    PromptSubmitted {
        prompt: String,
        #[serde(default)]
        session: Option<String>,
    },
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Answer {
    pub(crate) block: Option<String>,
}

// Claude Code's SessionStart hook gives its source as startup, resume, clear or compact.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SessionSource {
    Resume,
    #[default]
    #[serde(other)]
    Other,
}
