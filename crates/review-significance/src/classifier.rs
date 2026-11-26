use std::sync::Arc;

use crate::SignificanceResult;
use review_explore::Comparison;

#[derive(Clone, Default)]
pub struct JevClassifier(Option<Arc<dyn SignificanceClassifier>>);

impl std::fmt::Debug for JevClassifier {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("JevClassifier")
            .field(&self.is_enabled())
            .finish()
    }
}

impl JevClassifier {
    pub fn disabled() -> Self {
        Self(None)
    }

    pub fn enabled(classifier: Arc<dyn SignificanceClassifier>) -> Self {
        Self(Some(classifier))
    }

    pub fn is_enabled(&self) -> bool {
        self.0.is_some()
    }

    pub fn classifier(&self) -> Option<&dyn SignificanceClassifier> {
        self.0.as_deref()
    }
}

pub trait SignificanceClassifier: Send + Sync {
    fn rubric(&self) -> &str;

    fn plan(&self, comparison: &Comparison, classified: &dyn Fn(&str) -> bool) -> SignificancePlan;
}

type Classify = Box<dyn FnOnce(&mut dyn FnMut(SignificanceResult) -> bool) -> bool + Send>;

pub struct SignificancePlan {
    total_windows: usize,
    classify: Classify,
}

impl SignificancePlan {
    pub fn new(
        total_windows: usize,
        classify: impl FnOnce(&mut dyn FnMut(SignificanceResult) -> bool) -> bool + Send + 'static,
    ) -> Self {
        Self {
            total_windows,
            classify: Box::new(classify),
        }
    }

    pub fn total_windows(&self) -> usize {
        self.total_windows
    }

    pub fn run(self, mut record: impl FnMut(SignificanceResult) -> bool) -> bool {
        (self.classify)(&mut record)
    }
}
