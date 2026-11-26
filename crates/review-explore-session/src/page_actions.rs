use review_explore::{Command, DispatchState};
use review_explore_page::{CommandRefusal, Waiting};

use crate::ExploreSession;

impl ExploreSession {
    pub(crate) fn stop(&mut self, waiting: &Waiting) -> Result<(), CommandRefusal> {
        let waits = match waiting {
            Waiting::Start(start) => {
                self.state.round.is_none() && self.state.start.starting() == Some(start.as_str())
            }
            Waiting::Turn(request) => self.delivering(request),
        };
        if !waits {
            return Err(CommandRefusal::Stale);
        }
        let round = self
            .state
            .round
            .as_ref()
            .map(|round| round.exploration.instance.clone());
        let _ = self.events.send(ui_events::ExplorePageStopped { round });
        self.command(Command::Cancel);
        Ok(())
    }

    fn delivering(&self, request: &str) -> bool {
        self.state
            .pending
            .as_ref()
            .is_some_and(|(_, id)| id == request)
            && self.state.round.as_ref().is_some_and(|round| {
                round
                    .exploration
                    .pending_request()
                    .is_some_and(|pending| pending.request == request)
            })
    }

    pub(crate) fn retry_from_page(
        &mut self,
        request: &str,
        attempt: &str,
    ) -> Result<(), CommandRefusal> {
        let round = self.saved_round()?;
        if self.delivering(request) {
            return Err(CommandRefusal::AlreadyApplied);
        }
        let retried = round
            .exploration
            .retry_request()
            .is_some_and(|retry| retry.request == request)
            && round
                .turns
                .get(request)
                .is_some_and(|delivery| delivery.attempt == attempt);
        if !retried {
            return Err(CommandRefusal::Stale);
        }
        let mut exploration = round.exploration;
        exploration.pause_delivery();
        let request = exploration
            .retry()
            .map_err(|error| CommandRefusal::Failed(error.to_string()))?;
        self.retry(request).map_err(CommandRefusal::Failed)
    }

    pub(crate) fn cancel_answer_from_page(&mut self, answer: String) -> Result<(), CommandRefusal> {
        let offered = self.cancellable().is_some()
            && self.state.round.as_ref().is_some_and(|round| {
                round
                    .exploration
                    .answers
                    .iter()
                    .any(|kept| kept.id == answer)
            });
        if !offered {
            return Err(CommandRefusal::Stale);
        }
        self.cancel_answer(answer, crate::cancel::Cancel::From)
            .map_err(|error| CommandRefusal::Failed(error.to_string()))
    }

    pub(crate) fn reset_round(&mut self, round: &str) -> Result<(), CommandRefusal> {
        let shown = self
            .state
            .round
            .as_ref()
            .is_some_and(|shown| shown.exploration.instance == round);
        if !shown || self.state.storage_error.is_some() {
            return Err(CommandRefusal::Stale);
        }
        self.command(Command::Reset);
        let _ = self.events.send(ui_events::ExplorePageReset {
            round: round.to_owned(),
        });
        Ok(())
    }

    pub(crate) fn cancel_implementation_from_page(
        &mut self,
        delivery: &str,
    ) -> Result<(), CommandRefusal> {
        let round = self.saved_round()?;
        let sending = self.state.implementation.is_some()
            && round.implementations.get(delivery).is_some_and(|saved| {
                matches!(
                    saved.state,
                    DispatchState::Queued | DispatchState::Attempting
                )
            });
        if !sending {
            return Err(CommandRefusal::Stale);
        }
        self.command(Command::CancelImplementation);
        Ok(())
    }

    pub(crate) fn resend_from_page(
        &mut self,
        conclusion: &str,
        delivery: &str,
        attempt: &str,
    ) -> Result<(), CommandRefusal> {
        let round = self.saved_round()?;
        let latest = round.latest_implementation(conclusion);
        let resent = latest.is_some_and(|latest| {
            latest.request.delivery == delivery
                && match latest.state {
                    DispatchState::Queued | DispatchState::Attempting => {
                        self.state.implementation.is_some()
                    }
                    DispatchState::Delivered => true,
                    _ => false,
                }
        });
        if resent {
            return Err(CommandRefusal::AlreadyApplied);
        }
        let saved = latest
            .filter(|latest| {
                latest.request.delivery == delivery
                    && latest.attempt == attempt
                    && match latest.state {
                        DispatchState::Queued => self.state.implementation.is_none(),
                        DispatchState::NotStarted => true,
                        _ => false,
                    }
            })
            .map(|latest| latest.request.clone());
        let request = saved.ok_or(CommandRefusal::Stale)?;
        self.implement(request).map_err(CommandRefusal::Failed)
    }
}
