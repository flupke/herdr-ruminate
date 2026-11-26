use std::io::Read;

pub use fork_guard::SUBMITS;
use fork_guard::command_refusal;

// A Claude Code fork's tool list shapes its prompt cache, which disallowing tools would
// change: a `PreToolUse` hook leaves it as the agent's.
const READERS: &[&str] = &[
    "Read",
    "Grep",
    "Glob",
    "LS",
    "NotebookRead",
    "Agent",
    "Task",
    "TodoWrite",
    "ToolSearch",
    "WebFetch",
    "WebSearch",
    "BashOutput",
];

fn refusal(input: &serde_json::Value) -> Option<String> {
    let tool = input.get("tool_name")?.as_str()?;
    if tool.starts_with("mcp__") && !SUBMITS.contains(&tool) {
        return Some(format!(
            "{tool} is not allowed here: only submit_question and submit_conclusion."
        ));
    }
    if tool != "Bash" && !READERS.contains(&tool) && !SUBMITS.contains(&tool) {
        return Some(format!(
            "{tool} is not allowed: this session reads code and writes nothing."
        ));
    }
    if tool == "Bash" {
        let command = input
            .pointer("/tool_input/command")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        return command_refusal(command);
    }
    None
}

// Claude Code refuses a tool call whose `PreToolUse` hook exits 2, with the reason on
// standard error, and runs it on exit 0.
pub fn run_guard() -> i32 {
    let mut text = String::new();
    let _ = std::io::stdin().read_to_string(&mut text);
    let Ok(input) = serde_json::from_str::<serde_json::Value>(&text) else {
        eprintln!("The reviewer's guard could not read the tool call.");
        return 2;
    };
    match refusal(&input) {
        Some(reason) => {
            eprintln!("{reason}");
            2
        }
        None => 0,
    }
}

#[cfg(test)]
#[path = "guard.tests.rs"]
mod tests;
