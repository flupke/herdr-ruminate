use std::fmt;
use std::path::{Path, PathBuf};

use herdr_client::protocol::{Agent, AgentPort};

// herdr-bowtie writes this agent file under a folder of its --add-dir.
const AGENT_FILE: &str = ".claude/agents/bt-self-review.md";

// The agent name Herdr reports for Claude Code.
const CLAUDE: &str = "claude";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Effort {
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

impl Effort {
    fn parse(level: &str) -> Option<Self> {
        Some(match level {
            "low" => Self::Low,
            "medium" => Self::Medium,
            "high" => Self::High,
            "xhigh" => Self::Xhigh,
            "max" => Self::Max,
            _ => return None,
        })
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Xhigh => "xhigh",
            Self::Max => "max",
        }
    }
}

impl fmt::Display for Effort {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelEffort {
    pub model: String,
    pub effort: Effort,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExploreModels {
    pub round: ModelEffort,
    pub worker: Option<ModelEffort>,
}

impl ExploreModels {
    pub fn of_agent(port: &dyn AgentPort, agent: &Agent) -> Option<Self> {
        if agent.agent.as_deref() != Some(CLAUDE) {
            return None;
        }
        let processes = port.pane_process_info(&agent.pane_id).ok()?;
        processes.foreground_processes.iter().find_map(|process| {
            let argv = match &process.argv {
                Some(argv) => argv.clone(),
                None => command_line(process.pid)?,
            };
            Self::of_command(&argv, |path| std::fs::read_to_string(path).ok())
        })
    }

    fn of_command(argv: &[String], read: impl Fn(&Path) -> Option<String>) -> Option<Self> {
        let round = added_folders(argv)
            .iter()
            .find_map(|folder| ModelEffort::of_agent_file(&read(&folder.join(AGENT_FILE))?))?;
        let worker = option_value(argv, "--model")
            .and_then(|model| alias(&model))
            .zip(option_value(argv, "--effort").and_then(|effort| Effort::parse(&effort)))
            .map(|(model, effort)| ModelEffort {
                model: model.to_owned(),
                effort,
            });
        Some(Self { round, worker })
    }

    // Claude Code's own /model and /effort would save the model as the user's default.
    pub fn round_line(&self) -> String {
        format!(
            "Explore runs on {} at {} effort.",
            self.round.model, self.round.effort
        )
    }
}

impl ModelEffort {
    fn of_agent_file(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        if lines.next()?.trim_end() != "---" {
            return None;
        }
        let (mut model, mut effort) = (None, None);
        for line in lines.take_while(|line| line.trim_end() != "---") {
            match line.split_once(':') {
                Some(("model", value)) => model = Some(value.trim().trim_matches('"').to_owned()),
                Some(("effort", value)) => effort = Effort::parse(value.trim().trim_matches('"')),
                _ => {}
            }
        }
        let model =
            model.filter(|model| !model.is_empty() && !model.contains(char::is_whitespace))?;
        Some(Self {
            model,
            effort: effort?,
        })
    }
}

// Claude Code's Agent tool takes only these aliases as its model.
const ALIASES: [&str; 4] = ["sonnet", "opus", "haiku", "fable"];

fn alias(model: &str) -> Option<&'static str> {
    let family = model.strip_prefix("claude-").unwrap_or(model);
    let family = family.split(['-', '[']).next()?;
    ALIASES.into_iter().find(|alias| *alias == family)
}

// Claude Code takes the last value of an option given more than once.
fn option_value(argv: &[String], option: &str) -> Option<String> {
    let mut arguments = argv.iter().take_while(|argument| *argument != "--");
    let mut value = None;
    while let Some(argument) = arguments.next() {
        if argument == option {
            value = arguments
                .next()
                .filter(|value| !value.starts_with('-'))
                .cloned();
        } else if let Some(inline) = argument
            .strip_prefix(option)
            .and_then(|rest| rest.strip_prefix('='))
        {
            value = Some(inline.to_owned());
        }
    }
    value
}

fn command_line(pid: u32) -> Option<Vec<String>> {
    let bytes = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    Some(
        bytes
            .split(|byte| *byte == 0)
            .filter(|argument| !argument.is_empty())
            .map(|argument| String::from_utf8_lossy(argument).into_owned())
            .collect(),
    )
}

fn added_folders(argv: &[String]) -> Vec<PathBuf> {
    let mut folders = Vec::new();
    let mut adding = false;
    for argument in argv.iter().take_while(|argument| *argument != "--") {
        if let Some(folder) = argument.strip_prefix("--add-dir=") {
            folders.push(PathBuf::from(folder));
            adding = false;
        } else if argument.starts_with('-') {
            adding = argument == "--add-dir";
        } else if adding {
            folders.push(PathBuf::from(argument));
        }
    }
    folders
}

#[cfg(test)]
#[path = "lib.tests.rs"]
mod tests;
