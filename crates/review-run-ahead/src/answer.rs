use review_turn_path::TurnPath;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct AnswerRecord {
    pub question: String,
    pub version: u32,
    pub answer: String,
    pub request: String,
    pub at_ms: u64,
    pub path: TurnPath,
}
