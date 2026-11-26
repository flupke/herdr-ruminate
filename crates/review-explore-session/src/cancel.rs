use std::sync::Arc;

use review_explore::ExploreRound;

use crate::ExploreSession;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Cancel {
    Latest,
    From,
}

impl ExploreSession {
    pub(crate) fn cancel_answer(&mut self, answer: String, cancel: Cancel) -> Result<(), String> {
        let result = self
            .cancel_from(&answer, cancel)
            .map_err(|error| error.to_string());
        if result.is_ok() {
            self.run_ahead_discard(review_run_ahead::DiscardReason::AnswerCancelled);
        }
        let outcome = result.as_ref().map(|_| ()).map_err(Clone::clone);
        let _ = self
            .events
            .send(ui_events::ExploreAnswerCancelled { answer, result });
        outcome
    }

    fn cancel_from(&mut self, answer: &str, cancel: Cancel) -> eyre::Result<Arc<ExploreRound>> {
        eyre::ensure!(
            self.state.storage_error.is_none(),
            "Explore storage is unavailable: {}",
            self.state.storage_error.as_deref().unwrap_or_default()
        );
        eyre::ensure!(!self.state.historical, "This round is history");
        let shown = self
            .state
            .round
            .as_ref()
            .ok_or_else(|| eyre::eyre!("No Explore round"))?;
        let checkpoint = &shown.exploration.comparison.checkpoint;
        let round = self
            .rounds
            .round(&checkpoint.review_unit, &shown.exploration.instance)?
            .ok_or_else(|| eyre::eyre!("Saved Explore round is missing"))?;
        eyre::ensure!(
            cancel == Cancel::From
                || round
                    .exploration
                    .answers
                    .last()
                    .is_some_and(|latest| latest.id == answer),
            "Only the latest answer can be cancelled"
        );
        let cancelled = round.clone().cancel_answers_from(answer)?;
        let mut problems = std::collections::BTreeSet::new();
        for cancelled in &cancelled {
            let author = review_types::MarkAuthor::Explore {
                answer: cancelled.answer.id.clone(),
            };
            for marks in &cancelled.marks {
                problems.extend(self.unmark(&round, &author, marks)?);
            }
        }
        for problem in problems {
            let _ = self.events.send(ui_events::ToastRequested {
                text: format!("Cancelled answer: {problem}"),
                kind: toasts::ToastKind::Error,
            });
        }
        let checkpoint = &round.exploration.comparison.checkpoint;
        let updated = self.rounds.update(
            &checkpoint.review_unit,
            &round.exploration.instance,
            |round| {
                round
                    .cancel_answers_from(answer)
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            },
        );
        let ((), round) = match updated {
            Ok(updated) => updated,
            Err(error @ review_store::Error::ExploreRefused(_)) => return Err(error.into()),
            Err(error) => {
                self.state.storage_error = Some(error.to_string());
                let _ = self
                    .events
                    .send(ui_events::ExploreStorageFailed(error.to_string()));
                return Err(error.into());
            }
        };
        if let Some((_, request)) = &self.state.pending
            && cancelled
                .iter()
                .any(|cancelled| cancelled.request.as_ref() == Some(request))
        {
            self.state.prompt = None;
            self.state.pending = None;
        }
        self.state.grants_held = true;
        self.state.round = Some(round.clone());
        Ok(Arc::new(round))
    }
}
