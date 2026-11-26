use review_explore::TurnRequest;
use review_run_ahead::{DiscardReason, ForkPoint, PlainReason, TurnPath};

use super::switch::Switching;
use super::take::ForkFiles;
use super::{Asked, TakenFork};
use crate::ExploreSession;
use crate::turn::SavedTurn;

pub(super) struct Finishing {
    pub(super) asked: Asked,
    pub(super) asked_at_ms: u64,
    pub(super) turn: SavedTurn,
    pub(super) point: ForkPoint,
    pub(super) taken_at_ms: u64,
    pub(super) fork: TakenFork,
    pub(super) files: ForkFiles,
}

impl ExploreSession {
    pub(super) fn wait_for_fork(&mut self, finishing: Finishing) -> TurnPath {
        let request = &finishing.turn.request;
        self.state.prompt = None;
        self.state.pend(request);
        self.run_ahead.host.log(&format!(
            "{}: the fork for {} has not submitted its turn yet: the answer waits for it",
            finishing.asked.label(),
            finishing.fork.choice
        ));
        let _ = self.events.send(ui_events::ExploreTurnFinishing {
            round: request.instance.clone(),
            request: request.request.clone(),
            started_at_ms: finishing.taken_at_ms,
        });
        let session = finishing.fork.session.clone();
        self.run_ahead.finishing = Some(finishing);
        TurnPath::Prepared { session }
    }

    pub(crate) fn run_ahead_finishing_since(&self, request: &str) -> Option<u64> {
        if let Some(finishing) = &self.run_ahead.finishing
            && finishing.turn.request.request == request
        {
            return Some(finishing.taken_at_ms);
        }
        self.run_ahead
            .switching
            .as_ref()
            .filter(|switching| switching.turn.request.request == request)
            .and_then(Switching::finishing_since)
    }

    pub(super) fn run_ahead_fork_submitted(&mut self) {
        let Some(finishing) = self
            .run_ahead
            .finishing
            .take_if(|finishing| finishing.fork.kept.is_some())
        else {
            return;
        };
        let Finishing {
            asked,
            asked_at_ms,
            turn,
            point,
            taken_at_ms,
            fork,
            files,
        } = finishing;
        self.run_ahead.host.log(&format!(
            "{}: the fork the answer waited for submitted its turn",
            asked.label()
        ));
        let path = match self.unchanged_since_forks(&asked, asked_at_ms, &point, taken_at_ms) {
            Ok(()) => self.continue_as(Switching {
                asked: asked.clone(),
                dispatch: self.dispatch_of(&turn.request, &turn.attempt, None),
                turn: turn.clone(),
                fork,
                files,
                hold: self.prompts.hold(),
                finishing_since_ms: Some(taken_at_ms),
                stopped: false,
            }),
            Err(reason) => {
                self.discard_forks(
                    &asked.round,
                    std::slice::from_ref(&fork),
                    DiscardReason::Answered,
                );
                TurnPath::Plain { reason }
            }
        };
        if let TurnPath::Plain { reason } = path {
            self.answer_to_agent(&asked, turn, reason);
        }
    }

    pub(super) fn run_ahead_waited_fork_ended(&mut self, session: &str) {
        let Some(finishing) = self
            .run_ahead
            .finishing
            .take_if(|finishing| finishing.fork.session == session)
        else {
            return;
        };
        self.discard_finishing_fork(&finishing, DiscardReason::Answered);
        self.answer_to_agent(&finishing.asked, finishing.turn, PlainReason::NoTurn);
    }

    pub(crate) fn run_ahead_stop_waiting(&mut self) -> Option<TurnRequest> {
        const STOPPED: &str = "the reviewer stopped waiting for the fork: the answer goes to the \
                               agent in the pane";
        if let Some(finishing) = self.run_ahead.finishing.take() {
            self.drop_finishing(
                &finishing,
                (DiscardReason::StoppedWaiting, PlainReason::StoppedWaiting),
                STOPPED,
            );
            return Some(finishing.turn.request);
        }
        let switching = self
            .run_ahead
            .switching
            .as_mut()
            .filter(|switching| switching.finishing_since().is_some())?;
        switching.stopped = true;
        let (asked, request) = (switching.asked.clone(), switching.turn.request.clone());
        self.run_ahead.host.log(&format!(
            "{}: {STOPPED}, once the switch ends",
            asked.label()
        ));
        self.record_plain_after(&asked, &request.request, PlainReason::StoppedWaiting);
        Some(request)
    }

    pub(super) fn run_ahead_withdraw_finishing(&mut self) {
        let Some(finishing) = self.run_ahead.finishing.take_if(|finishing| {
            let request = &finishing.turn.request;
            !self.state.is_pending(&request.instance, &request.request)
        }) else {
            return;
        };
        self.drop_finishing(
            &finishing,
            (DiscardReason::Withdrawn, PlainReason::Withdrawn),
            "the answer that waited for the fork was withdrawn",
        );
    }

    fn drop_finishing(
        &mut self,
        finishing: &Finishing,
        (discard, path): (DiscardReason, PlainReason),
        line: &str,
    ) {
        self.run_ahead
            .host
            .log(&format!("{}: {line}", finishing.asked.label()));
        self.discard_finishing_fork(finishing, discard);
        self.record_plain_after(&finishing.asked, &finishing.turn.request.request, path);
    }

    fn discard_finishing_fork(&mut self, finishing: &Finishing, reason: DiscardReason) {
        self.discard_forks(
            &finishing.asked.round,
            std::slice::from_ref(&finishing.fork),
            reason,
        );
    }

    fn answer_to_agent(&mut self, asked: &Asked, turn: SavedTurn, reason: PlainReason) {
        self.run_ahead.host.log(&format!(
            "{}: the answer goes to the agent in the pane: {reason:?}",
            asked.label()
        ));
        self.record_plain_after(asked, &turn.request.request, reason);
        if !self.run_ahead_hold(&turn) {
            self.send_turn(turn);
        }
    }
}
