//! The reviewer's Claude Code plugin, and what its hooks run.
//!
//! The plugin (`plugin/`, listed by the marketplace of this directory, which `make install`
//! adds) runs `reviewer-control agent-hook` on each hook it registers, with Claude Code's event
//! on its standard input. In a pane of Herdr or a terminal of herdr-core, that hands the event
//! to the reviewers of the pane's host ([`agent_hooks`]), and a prompt a reviewer blocks does
//! not run; anywhere else, it does nothing. In a terminal of herdr-core, which keeps no agent
//! session, the hook also names the agent's session in the terminal's session label, and
//! removes it when the session ends (ADR 0016). Claude Code reads its hooks when it starts: an
//! agent started before the plugin was installed runs without them.

use std::io::{Read, Write};
use std::time::Duration;

use agent_fork::ProcessStamp;
use agent_hooks::{AgentEvent, HookDirectory, HookPane, HookedSession, Report, SessionSource};
use hc_client::HcClient;
use herdr_client::protocol::PaneId;
use serde::Deserialize;

/// The subcommand of `reviewer-control` that each hook of the plugin runs.
pub const SUBCOMMAND: &str = "agent-hook";

/// How long a submitted prompt waits for the reviewers' answer before it goes on: Claude Code
/// waits for the hook before it takes the prompt.
const ASK_LIMIT: Duration = Duration::from_millis(500);

/// The agent's name, as the reviewer and its session label give it.
const CLAUDE: &str = "claude";

/// Runs the hook whose event Claude Code writes on standard input, for the pane this process
/// runs in, if any, and prints the decision Claude Code reads, if one is made.
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

/// Where the hook reports: the agent's pane, the directory of the reviewers of its host, and,
/// in a terminal of herdr-core, the terminal's session label.
struct HookTarget<'a> {
    pane: &'a PaneId,
    directory: &'a HookDirectory,
    session_label: Option<&'a dyn SessionLabel>,
    /// Whether the agent leads the foreground process group of its terminal. A `claude` that a
    /// tool or an MCP server of the agent runs inherits the terminal's variables, but has no
    /// terminal or leads no group: its session is not the terminal's.
    leads_terminal: bool,
}

/// The session label of the agent's terminal of herdr-core.
trait SessionLabel {
    /// Names `session` as the agent's.
    fn report(&self, pane: &PaneId, session: &str) -> herdr_client::Result<()>;

    /// Removes the label if it names `session`, which ended.
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

/// The events of Claude Code's hooks that the reviewer reads, with the fields it reads.
#[derive(Deserialize)]
#[serde(tag = "hook_event_name")]
enum Payload {
    SessionStart {
        session_id: String,
        #[serde(default)]
        source: SessionSource,
        /// The subagent the session belongs to, if it belongs to one.
        #[serde(default)]
        agent_id: Option<String>,
    },
    SessionEnd {
        session_id: String,
        /// Why the session ended, such as `prompt_input_exit`, `clear` or `resume`.
        #[serde(default)]
        reason: String,
        /// The subagent the session belongs to, if it belongs to one.
        #[serde(default)]
        agent_id: Option<String>,
    },
    UserPromptSubmit {
        prompt: String,
        #[serde(default)]
        session_id: Option<String>,
    },
    /// An event the plugin does not register.
    #[serde(other)]
    Other,
}

/// Hands the event `payload` of the agent, the process `agent`, to `target`; returns the
/// decision Claude Code reads, when a reviewer blocks a prompt.
fn hook(target: &HookTarget<'_>, agent: u32, payload: &[u8]) -> Option<String> {
    let pane = target.pane;
    match serde_json::from_slice::<Payload>(payload).ok()? {
        // A subagent's session is not the agent's.
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
        // A cleared or resumed session's successor names itself at its start: removing the
        // label after that start would leave the agent without one.
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

/// How Claude Code opens a long paste in the prompt it reports, before the paste's ID.
const PASTE_OPEN: &str = "<pasted_content id=\"";

/// `prompt` with each long paste in it as it was pasted. Claude Code (2.1.309) reports a long
/// paste, such as a long prompt Herdr types, wrapped as `<pasted_content id="N">` and a line
/// break, the text, then `</pasted_content id="N">` and a line break.
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
