//! A fork's settings: the pane agent's own `--settings`, which shape its tools and so its prompt
//! cache, with the fork's guard as their only hook. Claude Code takes one `--settings`, the last
//! given, so a fork cannot pass the agent's beside its own. They go in a file only the user
//! reads, since a command line is everyone's to read.
//!
//! Claude Code 2.1.318 skips a settings file with an error as a whole, the guard with it: a
//! `SessionStart` hook of the same file writes [`GUARD_MARK`] on its standard error, which the
//! fork's stream reports before its `init`, and a fork whose stream lacks it is stopped there
//! (`crate::stream`).

use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// What the `SessionStart` hook of a fork's settings writes on its standard error: the fork runs
/// the settings that hold its guard.
pub(crate) const GUARD_MARK: &str = "herdr-reviewer-fork-guard";

/// The settings of the agent's `--settings` that a fork keeps: all but their hooks, which a
/// fork does not run, so that none answers a tool call in the guard's place. An agent without
/// `--settings` has none.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub(crate) struct AgentSettings(Map<String, Value>);

impl AgentSettings {
    /// The settings of the agent's `--settings` value `value`, JSON or the path of a JSON file,
    /// a relative one from the agent's directory `directory`; or why a fork cannot run its
    /// guard with them.
    pub(crate) fn read(value: &str, directory: &Path) -> Result<Self, String> {
        let text = if value.trim_start().starts_with('{') {
            value.to_owned()
        } else {
            fs::read_to_string(directory.join(value))
                .map_err(|error| format!("cannot read the agent's settings {value}: {error}"))?
        };
        let Ok(Value::Object(mut settings)) = serde_json::from_str(&text) else {
            // Inline settings may hold secrets: they go to the log and the Explore page.
            let named = if value.trim_start().starts_with('{') {
                "inline"
            } else {
                value
            };
            return Err(format!(
                "the agent's settings {named} are not a JSON object"
            ));
        };
        if settings.get("disableAllHooks") == Some(&Value::Bool(true)) {
            return Err("the agent's settings disable the hooks, and a fork's guard is one".into());
        }
        settings.remove("hooks");
        Ok(Self(settings))
    }

    /// The settings of a fork: these, with the hook that runs `guard` before each of its tools,
    /// and the one that marks them read. They outrank the user's and the project's, which
    /// could disable the hooks.
    pub(crate) fn with_guard(&self, guard: &str) -> String {
        let mut settings = self.0.clone();
        settings.insert("disableAllHooks".into(), Value::Bool(false));
        settings.insert(
            "hooks".into(),
            serde_json::json!({
                "PreToolUse": [
                    {"matcher": "*", "hooks": [{"type": "command", "command": guard}]}
                ],
                "SessionStart": [
                    {"hooks": [{"type": "command", "command": format!("echo {GUARD_MARK} >&2")}]}
                ],
            }),
        );
        Value::Object(settings).to_string()
    }
}

/// A fork's settings in a file that only the user may read, removed once dropped.
#[derive(Debug)]
pub(crate) struct SettingsFile(PathBuf);

impl SettingsFile {
    /// Writes `settings` for the fork `session` in the user's runtime directory, or in the
    /// temporary one without it.
    pub(crate) fn write(session: &str, settings: &str) -> std::io::Result<Self> {
        let directory = std::env::var_os("XDG_RUNTIME_DIR")
            .filter(|directory| !directory.is_empty())
            .map_or_else(std::env::temp_dir, PathBuf::from);
        Self::write_in(&directory, session, settings)
    }

    /// Writes `settings` for the fork `session` in `directory`.
    fn write_in(directory: &Path, session: &str, settings: &str) -> std::io::Result<Self> {
        let path = directory.join(format!("herdr-reviewer-fork-{session}.json"));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?;
        // Removed from here on, even when the write fails.
        let written = Self(path);
        file.write_all(settings.as_bytes())?;
        Ok(written)
    }

    /// Where the settings are, which the fork's `--settings` names.
    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for SettingsFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[cfg(test)]
#[path = "settings.tests.rs"]
mod tests;
