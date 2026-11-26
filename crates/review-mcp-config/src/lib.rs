mod document;
mod file;
mod user;

use std::str::FromStr;

pub use user::UserConfig;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Client {
    Codex,
    Claude,
    Antigravity,
}

impl Client {
    pub const ALL: [Self; 3] = [Self::Codex, Self::Claude, Self::Antigravity];

    fn file_name(self) -> &'static str {
        match self {
            Self::Codex => "config.toml",
            Self::Claude => ".claude.json",
            Self::Antigravity => "mcp_config.json",
        }
    }
}

impl FromStr for Client {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "codex" => Ok(Self::Codex),
            "claude" => Ok(Self::Claude),
            "antigravity" => Ok(Self::Antigravity),
            _ => Err("Expected codex, claude or antigravity".into()),
        }
    }
}

#[cfg(test)]
mod tests;
