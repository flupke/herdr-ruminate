use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct ExploreRoundSettings {
    pub writing: WritingStyle,
    pub run_ahead: RunAhead,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WritingStyle {
    Plain,
    #[default]
    SimplifiedTechnicalEnglish,
    Concise,
}

impl WritingStyle {
    pub fn is_plain(&self) -> bool {
        *self == Self::Plain
    }

    pub fn cycle(&mut self) {
        *self = match self {
            Self::Plain => Self::SimplifiedTechnicalEnglish,
            Self::SimplifiedTechnicalEnglish => Self::Concise,
            Self::Concise => Self::Plain,
        };
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunAhead {
    #[default]
    Off,
    Recommended,
    Every,
}

impl RunAhead {
    pub fn is_on(self) -> bool {
        self != Self::Off
    }

    pub fn prepares(self, recommended: bool) -> bool {
        match self {
            Self::Off => false,
            Self::Recommended => recommended,
            Self::Every => true,
        }
    }

    pub fn cycle(&mut self) {
        *self = match self {
            Self::Off => Self::Recommended,
            Self::Recommended => Self::Every,
            Self::Every => Self::Off,
        };
    }
}

#[cfg(test)]
#[path = "lib.tests.rs"]
mod tests;
