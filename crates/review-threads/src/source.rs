use std::sync::Arc;

use review_source::DiffRangeAnchor;
use serde::{Deserialize, Serialize};

use crate::{ReviewThread, ReviewThreads};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ThreadSource {
    pub anchor: DiffRangeAnchor,
    pub excerpt: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ThreadSubject {
    Round { round: String },
    Code(Arc<ThreadSource>),
}

impl ReviewThread {
    pub fn code(&self) -> Option<&ThreadSource> {
        match &self.subject {
            ThreadSubject::Code(source) => Some(source),
            ThreadSubject::Round { .. } => None,
        }
    }

    pub fn path(&self) -> Option<&str> {
        let anchor = &self.code()?.anchor;
        Some(
            anchor
                .new_path
                .as_deref()
                .or(anchor.old_path.as_deref())
                .unwrap_or(""),
        )
    }
}

impl ReviewThreads {
    pub fn sources_mut(&mut self) -> impl Iterator<Item = &mut Arc<ThreadSource>> {
        self.threads
            .iter_mut()
            .filter_map(|thread| match &mut thread.subject {
                ThreadSubject::Code(source) => Some(source),
                ThreadSubject::Round { .. } => None,
            })
    }
}
