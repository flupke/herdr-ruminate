use serde::{Deserialize, Serialize};

pub const MCP_SERVER: &str = "herdr_reviewer";

/// The stable identity of one logical review.
#[derive(
    Clone,
    Debug,
    Default,
    Deserialize,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
    schemars::JsonSchema,
)]
#[serde(transparent)]
pub struct ReviewUnit(String);

impl ReviewUnit {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<String> for ReviewUnit {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for ReviewUnit {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct QuestionNumber(usize);

impl From<usize> for QuestionNumber {
    fn from(number: usize) -> Self {
        Self(number)
    }
}

impl std::fmt::Display for QuestionNumber {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(output, "Q{}", self.0)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub struct TextEditorState {
    pub text: String,
    pub row: usize,
    pub column: usize,
    pub scroll: usize,
    pub normal: bool,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EditorKeymap {
    #[default]
    Vim,
    Regular,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MarkAuthor {
    #[default]
    Reviewer,
    Jev,
    Explore {
        answer: String,
    },
    ExploreRead {
        request: String,
    },
}
