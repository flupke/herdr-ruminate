use serde::{Deserialize, Serialize};

use crate::Exploration;

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct DiagramError {
    pub question: String,
    pub version: u32,
    pub source: String,
    pub message: String,
}

impl DiagramError {
    fn same_diagram(&self, other: &Self) -> bool {
        self.question == other.question
            && self.version == other.version
            && self.source == other.source
    }
}

impl Exploration {
    pub fn record_diagram_error(&mut self, error: DiagramError) -> eyre::Result<bool> {
        eyre::ensure!(
            self.posted(&error),
            "the round never posted version {} of question {}",
            error.version,
            error.question
        );
        match self
            .diagram_errors
            .iter_mut()
            .find(|kept| kept.same_diagram(&error))
        {
            Some(kept) if *kept == error => Ok(false),
            Some(kept) => {
                *kept = error;
                Ok(true)
            }
            None => {
                self.diagram_errors.push(error);
                Ok(true)
            }
        }
    }

    pub(crate) fn forget_diagram_errors_of_withdrawn_questions(&mut self) {
        let mut errors = std::mem::take(&mut self.diagram_errors);
        errors.retain(|error| self.posted(error));
        self.diagram_errors = errors;
    }

    fn posted(&self, error: &DiagramError) -> bool {
        self.questions
            .iter()
            .any(|question| question.is_version(&error.question, error.version))
    }
}
