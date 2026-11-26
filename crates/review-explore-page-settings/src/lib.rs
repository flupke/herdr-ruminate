use serde::{Deserialize, Serialize};

const DEFAULT_FIRST_PORT: u16 = 8790;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct ExplorePageSettings {
    pub pane_starts: PaneStarts,
    pub network: NetworkAccess,
}

impl ExplorePageSettings {
    pub fn set(&mut self, setting: ExplorePageSetting) {
        match setting {
            ExplorePageSetting::PaneStarts(starts) => self.pane_starts = starts,
            ExplorePageSetting::NetworkEnabled(enabled) => self.network.enabled = enabled,
            ExplorePageSetting::Interface(interface) => self.network.interface = interface,
            ExplorePageSetting::FirstPort(port) => self.network.first_port = port,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExplorePageSetting {
    PaneStarts(PaneStarts),
    NetworkEnabled(bool),
    Interface(Option<String>),
    FirstPort(u16),
}

impl ExplorePageSetting {
    pub fn interface(name: &str) -> Self {
        Self::Interface(interface_name(name))
    }

    pub fn first_port(text: &str) -> Result<Self, String> {
        let text = text.trim();
        match text.parse::<u16>() {
            Ok(port) if port > 0 => Ok(Self::FirstPort(port)),
            _ => Err(format!(
                "The first port must be a number from 1 to 65535, not {text:?}"
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PaneStarts {
    #[default]
    OnPage,
    InPane,
}

impl PaneStarts {
    pub fn toggle(&mut self) {
        *self = match self {
            Self::OnPage => Self::InPane,
            Self::InPane => Self::OnPage,
        };
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct NetworkAccess {
    pub enabled: bool,
    pub interface: Option<String>,
    pub first_port: u16,
}

impl Default for NetworkAccess {
    fn default() -> Self {
        Self {
            enabled: true,
            interface: None,
            first_port: DEFAULT_FIRST_PORT,
        }
    }
}

impl NetworkAccess {
    pub fn set_interface(&mut self, name: &str) {
        self.interface = interface_name(name);
    }
}

fn interface_name(name: &str) -> Option<String> {
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_owned())
}

#[cfg(test)]
#[path = "lib.tests.rs"]
mod tests;
