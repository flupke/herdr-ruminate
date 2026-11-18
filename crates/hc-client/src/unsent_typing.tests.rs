use herdr_client::protocol::{Agent, AgentStatus};
use test_case::test_case;

use super::waits_as_claude_code;

fn agent(name: &str, status: AgentStatus) -> Agent {
    serde_json::from_value(serde_json::json!({
        "pane_id": "t1",
        "tab_id": "tb1",
        "workspace_id": "sp1",
        "agent": name,
        "agent_status": status,
    }))
    .unwrap()
}

#[test_case("claude", AgentStatus::Idle, true; "an idle Claude Code")]
#[test_case("claude", AgentStatus::Done, true; "a Claude Code done with its turn")]
#[test_case("claude", AgentStatus::Working, false; "an agent at work")]
#[test_case("claude", AgentStatus::Blocked, false; "an agent that waits on its user")]
#[test_case("claude", AgentStatus::Unknown, false; "an agent whose status is unknown")]
#[test_case("codex", AgentStatus::Idle, false; "a Codex agent, whose composer is not read")]
fn a_key_goes_only_to_an_idle_claude_code(name: &str, status: AgentStatus, may: bool) {
    assert_eq!(waits_as_claude_code(&agent(name, status)), may);
}
