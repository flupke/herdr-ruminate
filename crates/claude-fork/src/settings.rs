use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

// Claude Code 2.1.318 skips a settings file with an error as a whole, hooks included; its
// stream reports a `SessionStart` hook's standard error before its `init`.
pub(crate) const GUARD_MARK: &str = "herdr-reviewer-fork-guard";

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub(crate) struct AgentSettings(Map<String, Value>);

impl AgentSettings {
    pub(crate) fn read(value: &str, directory: &Path) -> Result<Self, String> {
        let text = if value.trim_start().starts_with('{') {
            value.to_owned()
        } else {
            fs::read_to_string(directory.join(value))
                .map_err(|error| format!("cannot read the agent's settings {value}: {error}"))?
        };
        let Ok(Value::Object(mut settings)) = serde_json::from_str(&text) else {
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

    // Claude Code's `--settings` outrank the user's and the project's settings, whose
    // `disableAllHooks` could disable the hooks.
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

// Unix lets every user read a process's command line.
#[derive(Debug)]
pub(crate) struct SettingsFile(PathBuf);

impl SettingsFile {
    pub(crate) fn write(session: &str, settings: &str) -> std::io::Result<Self> {
        let directory = std::env::var_os("XDG_RUNTIME_DIR")
            .filter(|directory| !directory.is_empty())
            .map_or_else(std::env::temp_dir, PathBuf::from);
        Self::write_in(&directory, session, settings)
    }

    fn write_in(directory: &Path, session: &str, settings: &str) -> std::io::Result<Self> {
        let path = directory.join(format!("herdr-reviewer-fork-{session}.json"));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?;
        let written = Self(path);
        file.write_all(settings.as_bytes())?;
        Ok(written)
    }

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
