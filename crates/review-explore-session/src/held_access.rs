//! The Explore access the pane's agent holds: that of the latest prompt that reached it, or may
//! have, or of the fork it switched to. The round's access renews when a turn is posted, before
//! the turn reaches the agent, and some never do; after Cancel answer, which sends no prompt, the
//! round grants this one, so that the agent can revise the question the round went back to.

use std::sync::{Arc, Mutex, PoisonError};

/// Shared with the dispatch of each prompt, which sets it from the courier's thread once the
/// prompt's outcome is known.
#[derive(Clone, Debug, Default)]
pub(crate) struct HeldAccess(Arc<Mutex<Option<String>>>);

impl HeldAccess {
    /// The agent got `access`, or may have.
    pub(crate) fn hold(&self, access: &str) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(access.to_owned());
    }

    /// The access the agent holds, when this process gave it one.
    pub(crate) fn held(&self) -> Option<String> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}
