//! The models and the efforts of an Explore round of a Claude Code agent that herdr-bowtie
//! started: the round runs on herdr-bowtie's reviewer's, and its Implement on its worker's.
//!
//! herdr-bowtie writes the agent file of its self-review subagent, `.claude/agents/bt-self-review.md`,
//! with the reviewer's `model` and `effort` in its front matter, in a folder it gives the agent
//! with `--add-dir`, and starts the agent on the settings of its role, a worker's, with `--model`
//! and `--effort`. The reviewer reads both through the agent's command line. Each prompt of the
//! round names the reviewer's in one line ([`ExploreModels::round_line`]), which the hooks module
//! of the reviewer's Claude Code plugin reads (`crates/claude-hooks/plugin/hooks/register.ts`): it
//! sends every request of the agent's main loop on that model and effort from then on. Claude
//! Code's own `/model` and `/effort` would save them as the user's defaults. The Implement prompt
//! has a fresh subagent on the worker's do the work, through the Agent tool, whose `model` takes
//! only an alias ([`ExploreModels::worker`]).

use std::fmt;
use std::path::{Path, PathBuf};

use herdr_client::protocol::{Agent, AgentPort};

/// The agent file herdr-bowtie writes for an agent's self-review, under a folder of its
/// `--add-dir`.
const AGENT_FILE: &str = ".claude/agents/bt-self-review.md";

/// The agent Herdr reports for Claude Code: no other agent runs the plugin's hooks module.
const CLAUDE: &str = "claude";

/// A level of Claude Code's `--effort`.
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

/// A model, and the effort Claude Code runs it at.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelEffort {
    /// A model as Claude Code's `--model` takes it: an alias or a full ID, one word.
    pub model: String,
    pub effort: Effort,
}

/// The models and the efforts of an Explore round of a Claude Code agent that herdr-bowtie
/// started.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExploreModels {
    /// herdr-bowtie's reviewer's, which the round runs on.
    pub round: ModelEffort,
    /// The agent's own, as it started, its role's in herdr-bowtie, a worker's, which the subagent
    /// that implements the round's conclusion runs on: its model as the Agent tool takes it, an
    /// alias (`opus` for `claude-opus-5-5`), which runs the family's current model. None when its
    /// command line names no model of a known family, or no effort: the agent then implements
    /// the conclusion itself.
    pub worker: Option<ModelEffort>,
}

impl ExploreModels {
    /// The models of a round of `agent`, when `agent` is Claude Code and a process of its pane
    /// names a folder that holds herdr-bowtie's self-review agent file; none otherwise, and the
    /// round runs on the agent's own model.
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

    /// The models of a round of a Claude Code started with `argv`, whose files `read` reads:
    /// the round's from the first folder of its `--add-dir` that holds a self-review agent file
    /// with a model and an effort.
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

    /// The line each prompt of the round ends with, which the hooks module of the reviewer's
    /// Claude Code plugin reads.
    pub fn round_line(&self) -> String {
        format!(
            "Explore runs on {} at {} effort.",
            self.round.model, self.round.effort
        )
    }
}

impl ModelEffort {
    /// The model and the effort in the front matter of the agent file `text`.
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

/// The aliases of the Agent tool's `model`, each a family of models.
const ALIASES: [&str; 4] = ["sonnet", "opus", "haiku", "fable"];

/// The alias of `model`, as the Agent tool's `model` takes it: the model's family, such as
/// `opus` for `claude-opus-5-5` or `opus`; none for a model of another family.
fn alias(model: &str) -> Option<&'static str> {
    let family = model.strip_prefix("claude-").unwrap_or(model);
    let family = family.split(['-', '[']).next()?;
    ALIASES.into_iter().find(|alias| *alias == family)
}

/// The value of `option` in `argv`, before a prompt (`--`): the argument after it, or its inline
/// value; the last one, as Claude Code takes it, when `option` comes more than once.
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

/// The command line of the process `pid`, as Linux keeps it.
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

/// The folders `argv` adds with `--add-dir`, which takes every argument up to the next option,
/// or one inline value; what follows `--` is a prompt.
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
