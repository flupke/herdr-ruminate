use std::io::{Read, Write};
use std::time::Duration;

use agent_hooks::{AgentEvent, HookRoot, Report};
use serde::Deserialize;

pub const SUBCOMMAND: &str = "codex-hook";

// Codex waits for the hook before it takes the prompt.
const ASK_LIMIT: Duration = Duration::from_millis(500);

// Under Codex's app-server daemon, a hook's HERDR_* variables name the pane and the Herdr
// server of the terminal that started the daemon, not the agent's.
pub fn run_hook() {
    let mut payload = Vec::new();
    // Read whole, so that Codex never writes the event to a closed pipe.
    if std::io::stdin().read_to_end(&mut payload).is_err() {
        return;
    }
    let Some(root) = HookRoot::from_env() else {
        return;
    };
    if let Some(decision) = hook(&root, &payload) {
        let _ = std::io::stdout().write_all(decision.as_bytes());
    }
}

#[derive(Deserialize)]
#[serde(tag = "hook_event_name")]
enum Payload {
    UserPromptSubmit {
        session_id: String,
        prompt: String,
    },
    PreToolUse {
        session_id: String,
        tool_name: String,
        #[serde(default)]
        tool_input: serde_json::Value,
    },
    #[serde(other)]
    Other,
}

fn hook(root: &HookRoot, payload: &[u8]) -> Option<String> {
    match serde_json::from_slice::<Payload>(payload).ok()? {
        Payload::UserPromptSubmit { session_id, prompt } => {
            prompt_submitted(root, session_id, prompt)
        }
        Payload::PreToolUse {
            session_id,
            tool_name,
            tool_input,
        } => {
            if !root.is_codex_fork(&session_id) {
                return None;
            }
            let reason = crate::guard::refusal(&tool_name, &tool_input)?;
            Some(
                serde_json::json!({"hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "deny",
                    "permissionDecisionReason": reason,
                }})
                .to_string(),
            )
        }
        Payload::Other => None,
    }
}

fn prompt_submitted(root: &HookRoot, session_id: String, prompt: String) -> Option<String> {
    let _ = root.record_codex_session(&session_id);
    let report = Report {
        pane: None,
        event: AgentEvent::PromptSubmitted {
            prompt,
            session: Some(session_id),
        },
    };
    let reason = root.ask(&report, ASK_LIMIT)?;
    Some(serde_json::json!({"decision": "block", "reason": reason}).to_string())
}

#[cfg(test)]
#[path = "hook.tests.rs"]
mod tests;
