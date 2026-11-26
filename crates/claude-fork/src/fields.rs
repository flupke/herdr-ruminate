use std::path::{Path, PathBuf};

use agent_fork::{ForkCommand, ProcessStamp};
use review_run_ahead::HostFields;

use crate::settings::AgentSettings;

const TRANSCRIPTS: &str = "transcripts";
const PROCESS: &str = "process";
const COMMAND: &str = "command";
const SETTINGS: &str = "settings";

pub(crate) fn point(
    transcripts: &Path,
    command: &ForkCommand,
    settings: &AgentSettings,
) -> HostFields {
    let mut fields = HostFields::default();
    fields.set(TRANSCRIPTS, transcripts);
    fields.set(COMMAND, command);
    fields.set(SETTINGS, settings);
    fields
}

pub(crate) fn fork(transcripts: PathBuf) -> HostFields {
    let mut fields = HostFields::default();
    fields.set(TRANSCRIPTS, transcripts);
    fields
}

pub(crate) fn started(process: ProcessStamp) -> HostFields {
    let mut fields = HostFields::default();
    fields.set(PROCESS, process);
    fields
}

pub(crate) fn transcripts(fields: &HostFields) -> Option<PathBuf> {
    fields.get(TRANSCRIPTS)
}

pub(crate) fn command(fields: &HostFields) -> Option<ForkCommand> {
    fields.get(COMMAND)
}

pub(crate) fn settings(fields: &HostFields) -> Option<AgentSettings> {
    fields.get(SETTINGS)
}

pub fn process(fields: &HostFields) -> Option<ProcessStamp> {
    fields.get(PROCESS)
}

#[cfg(test)]
#[path = "fields.tests.rs"]
mod tests;
