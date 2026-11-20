use serde::Serialize;

use crate::{Resolution, ReviewThread};

/// A wakeup for a review's pending comments that did not reach the agent. The comments it
/// covered stay pending until a reply takes them up; the reviewer may retry.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WakeupFailure {
    /// The latest comment the wakeup covered, by its sequence: every pending comment up to
    /// it waits for a wakeup that reaches the agent.
    pub through: u64,
    /// Why the wakeup did not reach the agent.
    pub error: String,
}

/// A wakeup for a review's pending comments that waits before the agent's host types it: the
/// agent's input line shows text someone typed, or no input line shows. It goes once the line
/// reads empty, however long that takes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WakeupWait {
    /// The latest comment the wakeup covers, by its sequence.
    pub through: u64,
    /// Why it waits.
    pub reason: String,
}

/// What the reviewer shows of the latest wakeup for a review's pending comments, under the
/// threads it covers: why it waits to reach the agent, or why it did not reach it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum WakeupNotice {
    Waiting(WakeupWait),
    Failed(WakeupFailure),
}

impl WakeupNotice {
    /// The latest comment the wakeup covers, by its sequence.
    fn through(&self) -> u64 {
        match self {
            Self::Waiting(wait) => wait.through,
            Self::Failed(failure) => failure.through,
        }
    }

    /// Whether the wakeup covers the comment of `thread` that waits for a reply: the thread is
    /// open, and its last comment, unanswered, came no later than the wakeup's latest.
    pub fn covers(&self, thread: &ReviewThread) -> bool {
        let through = self.through();
        thread.resolution == Resolution::Open
            && thread.is_waiting()
            && thread
                .last_comment()
                .is_some_and(|comment| comment.sequence() <= through)
    }

    /// The notice as the reviewer reads it.
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
