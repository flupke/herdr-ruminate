use std::sync::{Arc, Mutex, PoisonError};

#[derive(Clone, Debug, Default)]
pub(crate) struct HeldAccess(Arc<Mutex<Option<String>>>);

impl HeldAccess {
    pub(crate) fn hold(&self, access: &str) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(access.to_owned());
    }

    pub(crate) fn held(&self) -> Option<String> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}
