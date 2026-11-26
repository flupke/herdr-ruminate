use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;

use review_explore_runner::TurnInstructions;
use tempfile::TempDir;

#[derive(Debug)]
pub(crate) struct TurnFiles {
    _directory: TempDir,
    prompt: String,
}

impl TurnFiles {
    pub(crate) fn write(turn: &TurnInstructions) -> eyre::Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix("herdr-review-turn-")
            .permissions(Permissions::from_mode(0o700))
            .tempdir()?;
        for file in turn.files() {
            std::fs::write(directory.path().join(file.name), &file.text)?;
        }
        let prompt = turn.prompt(directory.path());
        Ok(Self {
            _directory: directory,
            prompt,
        })
    }

    pub(crate) fn prompt(&self) -> &str {
        &self.prompt
    }
}

#[cfg(test)]
#[path = "turn_files.tests.rs"]
mod tests;
