use fork_guard::{SUBMITS, command_refusal};
use serde_json::Value;

// The names Codex 0.162.1 gives these tools in a hook's event.
const READERS: &[&str] = &["view_image", "get_goal", "clockcurr_time", "clocksleep"];

pub(crate) fn refusal(tool: &str, input: &Value) -> Option<String> {
    if SUBMITS.contains(&tool) || READERS.contains(&tool) {
        return None;
    }
    if tool == "Bash" {
        let command = input
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or_default();
        return command_refusal(command);
    }
    Some(if tool.starts_with("mcp__") {
        format!("{tool} is not allowed here: only submit_question and submit_conclusion.")
    } else {
        format!("{tool} is not allowed: this session reads code and writes nothing.")
    })
}

#[cfg(test)]
#[path = "guard.tests.rs"]
mod tests;
