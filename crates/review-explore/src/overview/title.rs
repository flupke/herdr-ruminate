use serde::Serialize;
use ts_rs::TS;

use super::steps::{Activity, Current, Position};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TabTitle {
    YourTurn { question: usize },
    AgentWorking,
    RetryNeeded,
    Conclusion,
}

impl TabTitle {
    pub(super) fn of(position: &Position<'_>) -> Self {
        match (position.activity, position.current) {
            (Activity::Working, _) => Self::AgentWorking,
            (Activity::Interrupted, _) | (Activity::Idle, Current::Design) => Self::RetryNeeded,
            (Activity::Idle, Current::Question(question)) => Self::YourTurn { question },
            (Activity::Idle, Current::Quiz | Current::Conclusion) => Self::Conclusion,
        }
    }
}
