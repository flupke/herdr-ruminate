use serde::Serialize;

use crate::{Resolution, ReviewThread};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WakeupFailure {
    pub through: u64,
    pub error: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WakeupWait {
    pub through: u64,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum WakeupNotice {
    Waiting(WakeupWait),
    Failed(WakeupFailure),
}

impl WakeupNotice {
    fn through(&self) -> u64 {
        match self {
            Self::Waiting(wait) => wait.through,
            Self::Failed(failure) => failure.through,
        }
    }

    pub fn covers(&self, thread: &ReviewThread) -> bool {
        let through = self.through();
        thread.resolution == Resolution::Open
            && thread.is_waiting()
            && thread
                .last_comment()
                .is_some_and(|comment| comment.sequence() <= through)
    }

    pub fn text(&self) -> String {
        match self {
            Self::Waiting(wait) => format!("Waiting to reach the agent: {}", wait.reason),
            Self::Failed(failure) => format!("Not delivered: {}", failure.error),
        }
    }
}

#[cfg(test)]
#[path = "wakeup.tests.rs"]
mod tests;
