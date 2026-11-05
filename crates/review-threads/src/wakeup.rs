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

impl WakeupFailure {
    /// Whether the wakeup covered the comment of `thread` that waits for a reply: the thread is
    /// open, and its last comment, unanswered, came no later than the wakeup's latest.
    pub fn covers(&self, thread: &ReviewThread) -> bool {
        thread.resolution == Resolution::Open
            && thread.is_waiting()
            && thread
                .last_comment()
                .is_some_and(|comment| comment.sequence() <= self.through)
    }
}

#[cfg(test)]
#[path = "wakeup.tests.rs"]
mod tests;
