use std::io::{Read, Write};
use std::time::Duration;

use agent_fork::ProcessStamp;
use agent_hooks::{AgentEvent, HookDirectory, HookPane, HookedSession, Report, SessionSource};
use hc_client::HcClient;
use herdr_client::protocol::PaneId;
use serde::Deserialize;

pub const SUBCOMMAND: &str = "agent-hook";

// Claude Code waits for the hook before it takes the prompt.
const ASK_LIMIT: Duration = Duration::from_millis(500);

const CLAUDE: &str = "claude";

pub fn run_hook() {
    let mut payload = Vec::new();
    // Read whole, so that Claude Code never writes to a closed pipe.
    if std::io::stdin().read_to_end(&mut payload).is_err() {
        return;
    }
    let Some(pane) = HookPane::from_env() else {
        return;
    };
    // Claude Code runs the hook itself, without a shell: its parent is the agent.
    let agent = std::os::unix::process::parent_id();
    let core = pane.core.map(HcClient::new);
    let target = HookTarget {
        pane: &pane.pane,
        directory: &pane.directory,
        session_label: core.as_ref().map(|core| core as &dyn SessionLabel),
        leads_terminal: agent_fork::leads_terminal_foreground(agent),
    };
    if let Some(decision) = hook(&target, agent, &payload) {
        let _ = std::io::stdout().write_all(decision.as_bytes());
    }
}

struct HookTarget<'a> {
    pane: &'a PaneId,
    directory: &'a HookDirectory,
    session_label: Option<&'a dyn SessionLabel>,
    // A `claude` that a tool or an MCP server of the agent runs inherits the terminal's
    // variables, but has no terminal or leads no process group of it.
    leads_terminal: bool,
}

// herdr-core keeps no agent session: the hook names it in the terminal's session label.
trait SessionLabel {
    fn report(&self, pane: &PaneId, session: &str) -> herdr_client::Result<()>;

    fn forget(&self, pane: &PaneId, session: &str) -> herdr_client::Result<()>;
}

impl SessionLabel for HcClient {
    fn report(&self, pane: &PaneId, session: &str) -> herdr_client::Result<()> {
        self.report_session(pane, CLAUDE, session)
    }

    fn forget(&self, pane: &PaneId, session: &str) -> herdr_client::Result<()> {
        self.forget_session(pane, CLAUDE, session)
    }
}

#[derive(Deserialize)]
#[serde(tag = "hook_event_name")]
enum Payload {
    SessionStart {
        session_id: String,
        #[serde(default)]
        source: SessionSource,
        #[serde(default)]
        agent_id: Option<String>,
    },
    SessionEnd {
        session_id: String,
        #[serde(default)]
        reason: String,
        #[serde(default)]
        agent_id: Option<String>,
    },
    UserPromptSubmit {
        prompt: String,
        #[serde(default)]
        session_id: Option<String>,
    },
    #[serde(other)]
    Other,
}

fn hook(target: &HookTarget<'_>, agent: u32, payload: &[u8]) -> Option<String> {
    let pane = target.pane;
    match serde_json::from_slice::<Payload>(payload).ok()? {
        Payload::SessionStart {
            agent_id: Some(_), ..
        }
        | Payload::SessionEnd {
            agent_id: Some(_), ..
        }
        | Payload::Other => None,
        Payload::SessionStart {
            session_id, source, ..
        } => {
            if let Some(label) = target.session_label
                && target.leads_terminal
            {
                let _ = label.report(pane, &session_id);
            }
            let started = HookedSession {
                session: session_id.clone(),
                process: ProcessStamp::read(agent),
            };
            let _ = target.directory.record(pane, &started);
            target.directory.tell(&Report {
                pane: Some(pane.clone()),
                event: AgentEvent::SessionStarted {
                    session: session_id,
                    source,
                },
            });
            None
        }
        Payload::SessionEnd {
            session_id, reason, ..
        } => {
            if let Some(label) = target.session_label
                && !matches!(reason.as_str(), "clear" | "resume")
            {
                let _ = label.forget(pane, &session_id);
            }
            None
        }
        Payload::UserPromptSubmit { prompt, session_id } => {
            let report = Report {
                pane: Some(pane.clone()),
                event: AgentEvent::PromptSubmitted {
                    prompt: unwrap_pastes(&prompt),
                    session: session_id,
                },
            };
            let reason = target.directory.ask(&report, ASK_LIMIT)?;
            Some(serde_json::json!({"decision": "block", "reason": reason}).to_string())
        }
    }
}

const PASTE_OPEN: &str = "<pasted_content id=\"";

// Claude Code 2.1.309 reports a long paste, such as a long prompt Herdr types, as
// `<pasted_content id="N">` and a line break, the text, then `</pasted_content id="N">`
// and a line break.
pub fn unwrap_pastes(prompt: &str) -> String {
    let mut text = String::with_capacity(prompt.len());
    let mut rest = prompt;
    while let Some(start) = rest.find(PASTE_OPEN) {
        let Some((id, body)) = rest[start + PASTE_OPEN.len()..].split_once("\">") else {
            break;
        };
        let Some((pasted, after)) = body.split_once(&format!("</pasted_content id=\"{id}\">"))
        else {
            break;
        };
        text.push_str(&rest[..start]);
        text.push_str(pasted.strip_prefix('\n').unwrap_or(pasted));
        rest = after.strip_prefix('\n').unwrap_or(after);
    }
    text.push_str(rest);
    text
}

#[cfg(test)]
#[path = "lib.tests.rs"]
mod tests;
