use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;

use agent_fork::ForkCommand;

use crate::arguments;
use crate::settings::AgentSettings;
use crate::transcripts::Transcripts;

#[derive(Clone, Debug)]
pub(crate) struct PaneClaude {
    pub(crate) command: ForkCommand,
    pub(crate) model: Option<String>,
    pub(crate) settings: AgentSettings,
}

impl PaneClaude {
    pub(crate) fn read(pid: u32) -> Result<Self, String> {
        let proc = PathBuf::from(format!("/proc/{pid}"));
        let read = |what: &str| format!("cannot read the agent's {what}");
        let program = fs::read_link(proc.join("exe")).map_err(|_| read("program"))?;
        let directory = fs::read_link(proc.join("cwd")).map_err(|_| read("directory"))?;
        let command = fs::read(proc.join("cmdline")).map_err(|_| read("command line"))?;
        let environment = process_environment::read(pid).map_err(|_| read("environment"))?;
        let command: Vec<String> = command
            .split(|byte| *byte == 0)
            .filter(|argument| !argument.is_empty())
            .map(|argument| String::from_utf8_lossy(argument).into_owned())
            .collect();
        // Unix: a script started through its interpreter has the interpreter as its program, then
        // the script.
        let script = command
            .get(1)
            .filter(|first| !first.starts_with('-') && directory.join(first).is_file());
        let rest = command
            .get(1 + usize::from(script.is_some())..)
            .unwrap_or_default();
        let kept = arguments::kept(rest)?;
        let settings = kept
            .settings
            .map(|value| AgentSettings::read(&value, &directory))
            .transpose()?
            .unwrap_or_default();
        Ok(Self {
            command: ForkCommand {
                program,
                arguments: script
                    .cloned()
                    .into_iter()
                    .chain(kept.arguments)
                    .map(OsString::from)
                    .collect(),
                directory,
                environment,
            },
            model: kept.model,
            settings,
        })
    }

    // Claude Code keeps its transcripts in `projects` under its configuration directory.
    pub(crate) fn transcripts(&self) -> Transcripts {
        let variable = |name: &str| {
            self.command
                .environment
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| PathBuf::from(value))
        };
        let configuration = variable("CLAUDE_CONFIG_DIR")
            .or_else(|| variable("HOME").map(|home| home.join(".claude")))
            .unwrap_or_else(|| PathBuf::from("/nonexistent/.claude"));
        Transcripts::at(configuration.join("projects"))
    }
}

#[cfg(test)]
#[path = "pane.tests.rs"]
mod tests;
