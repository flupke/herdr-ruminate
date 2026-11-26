use std::fs;
use std::path::{Path, PathBuf};

// Herdr prefers a local override to the agent detection rules built into its binary.
pub(crate) struct DetectionRules {
    agent: &'static str,
    manifest: &'static str,
}

impl DetectionRules {
    pub(crate) const ALL: [Self; 2] = [
        Self {
            agent: "claude",
            manifest: include_str!("../agent-detection/claude.toml"),
        },
        Self {
            agent: "codex",
            manifest: include_str!("../agent-detection/codex.toml"),
        },
    ];

    #[cfg(test)]
    pub(crate) fn agent(&self) -> &str {
        self.agent
    }

    pub(crate) fn path(&self, herdr_config_directory: &Path) -> PathBuf {
        herdr_config_directory
            .join("agent-detection")
            .join(format!("{}.toml", self.agent))
    }

    pub(crate) fn install(&self, herdr_config_directory: &Path) {
        let path = self.path(herdr_config_directory);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, self.manifest).unwrap();
    }
}
