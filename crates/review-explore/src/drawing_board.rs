use crate::{Exploration, InterviewUpdate};

impl Exploration {
    pub(crate) fn validate_drawing_board(&self, update: &InterviewUpdate) -> eyre::Result<()> {
        if !self
            .outstanding
            .as_ref()
            .is_some_and(crate::TurnRequest::at_drawing_board)
        {
            return Ok(());
        }
        let Some(conclusion) = &update.conclusion else {
            eyre::bail!(
                "The reviewer sent you back to the drawing board: conclude with submit_conclusion"
            );
        };
        eyre::ensure!(
            !conclusion.to_be_implemented.trim().is_empty(),
            "List the changes the design needs in to_be_implemented"
        );
        eyre::ensure!(
            conclusion.quiz.is_empty(),
            "The quiz waits for the new design: leave quiz empty"
        );
        eyre::ensure!(
            update.interpretation.is_none(),
            "Back to the drawing board decides no topic: leave interpretation null"
        );
        eyre::ensure!(
            update.reviewed.is_empty()
                && update.reopened.is_empty()
                && update.not_relevant.is_empty(),
            "The code will change: leave reviewed, reopened and not_relevant empty"
        );
        Ok(())
    }

    pub fn concluded_at_drawing_board(&self) -> bool {
        self.conclusion.is_some()
            && self
                .answers
                .last()
                .is_some_and(|answer| answer.drawing_board)
    }
}
