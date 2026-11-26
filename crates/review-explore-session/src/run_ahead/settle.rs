use herdr_client::protocol::{Agent, PaneId};
use review_explore::ConversationBinding;
use review_run_ahead::{Continuation, DiscardReason, ForkRecord, SwitchFailure, TurnPath};
use review_thread_service::{PinnedAgent, PromptHold};

use super::{Event, Move, RoundKey, RunAheadInput};
use crate::{ExploreSession, Input};

pub(super) struct Settling {
    round: RoundKey,
    fork: ForkRecord,
    target: Target,
    reason: String,
    trigger: Trigger,
    _hold: PromptHold,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Trigger {
    Prompt,
    RunAhead,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Target {
    Origin,
    Fork,
}

impl ExploreSession {
    pub(super) fn settle(
        &mut self,
        round: &RoundKey,
        fork: ForkRecord,
        pane: &PaneId,
        reason: String,
        trigger: Trigger,
    ) -> bool {
        if self.run_ahead.settling.is_some() {
            return true;
        }
        let target = if self.round_took(round, &fork) {
            Target::Fork
        } else {
            Target::Origin
        };
        let inbox = self.inbox.clone();
        let (key, session) = (round.clone(), fork.session.clone());
        let done = Box::new(move |result| {
            inbox.deliver(Input::RunAhead(RunAheadInput(Event::Settled {
                round: key,
                fork: session,
                result: Box::new(result),
            })));
        });
        let to = match target {
            Target::Origin => {
                let Some(from) = &fork.from else {
                    self.run_ahead.host.log(&format!(
                        "the agent may run the session of the fork {}, whose origin is not \
                         recorded: it stays where it is",
                        fork.session
                    ));
                    return false;
                };
                self.run_ahead.host.log(&format!(
                    "{reason}: the agent goes back to the session {from} it ran before the fork {}",
                    fork.session
                ));
                Move::resume(from.clone(), fork.trace())
            }
            Target::Fork => {
                self.run_ahead.host.log(&format!(
                    "{reason}: the round took the turn of the fork {}, whose session the agent \
                     resumes",
                    fork.session
                ));
                Move::switch(fork.trace())
            }
        };
        let hold = self.prompts.hold();
        to.start_once_drained(&self.run_ahead.host, &hold, pane, done);
        self.run_ahead.settling = Some(Settling {
            round: round.clone(),
            fork,
            target,
            reason,
            trigger,
            _hold: hold,
        });
        true
    }

    fn round_took(&self, round: &RoundKey, fork: &ForkRecord) -> bool {
        let Ok(forks) = self.rounds.forks(&round.unit, &round.instance) else {
            return false;
        };
        let Some(request) = forks.continued_as(&fork.session) else {
            return false;
        };
        self.rounds
            .round(&round.unit, &round.instance)
            .ok()
            .flatten()
            .is_some_and(|saved| {
                saved
                    .exploration
                    .conversation
                    .iter()
                    .any(|turn| turn.update.request == request)
            })
    }

    pub(super) fn unsettled(&self) -> Option<(RoundKey, ForkRecord)> {
        let saved = self.state.round.as_ref()?;
        let round = RoundKey {
            unit: saved.exploration.comparison.checkpoint.review_unit.clone(),
            instance: saved.exploration.instance.clone(),
        };
        let switching = self
            .run_ahead
            .switching
            .as_ref()
            .map(|switching| switching.fork.session.as_str());
        let reviewer = self.run_ahead.reviewer;
        let fork = self
            .rounds
            .forks(&round.unit, &round.instance)
            .ok()?
            .forks
            .into_iter()
            .find(|fork| {
                fork.is_unsettled()
                    && switching != Some(fork.session.as_str())
                    && !self.run_ahead.left_unrecorded.contains(&fork.session)
                    && fork.may_be_settled_by(reviewer)
            })?;
        Some((round, fork))
    }

    pub(super) fn settle_before(&mut self, turn: &crate::turn::SavedTurn) -> bool {
        if self.run_ahead.settling.is_none() {
            let Some((round, fork)) = self.unsettled() else {
                return false;
            };
            let Some(agent) = self.round_agent() else {
                return false;
            };
            if !self.can_settle(&round, &fork, &agent) {
                return false;
            }
            let pane = agent.pane_id;
            let reason = format!(
                "a prompt waits while the agent may run the session of the fork {}",
                fork.session
            );
            if !self.settle(&round, fork, &pane, reason, Trigger::Prompt) {
                return false;
            }
        }
        self.run_ahead
            .host
            .log("a turn waits for the agent to settle before its prompt goes out");
        self.state.pend(&turn.request);
        self.state.prompt = None;
        self.run_ahead.held = Some(turn.clone());
        true
    }

    pub(super) fn can_settle(
        &mut self,
        round: &RoundKey,
        fork: &ForkRecord,
        agent: &Agent,
    ) -> bool {
        if self.run_ahead.host.can_move(agent) {
            return true;
        }
        if let Some(why) = self.run_ahead.host.unhooked(agent) {
            self.agent_restarted(round, fork, &why);
        } else {
            self.run_ahead.host.log(&format!(
                "the agent may run the session of the fork {}, and cannot be moved: it stays \
                 where it is",
                fork.session
            ));
        }
        false
    }

    fn agent_restarted(&mut self, round: &RoundKey, fork: &ForkRecord, why: &str) {
        let now = review_explore::now_ms();
        let saved = self.update_forks(round, |forks| {
            if let Some(record) = forks.fork_mut(&fork.session) {
                record.continued = Some(Continuation::Restarted { at_ms: now });
                record.discard(DiscardReason::Answered, now);
            }
        });
        if let Err(error) = saved {
            self.run_ahead
                .host
                .log(&format!("the agent's restart was not recorded: {error}"));
            self.run_ahead.left_unrecorded.insert(fork.session.clone());
        }
        self.discard_in_background(round, fork.trace());
        self.run_ahead.host.log(&format!(
            "the agent runs without the hooks a move needs since it was told to resume the \
             session of the fork {}, which is discarded: {why}",
            fork.session
        ));
    }

    pub(super) fn run_ahead_settled(
        &mut self,
        round: &RoundKey,
        fork: &str,
        result: Result<Agent, SwitchFailure>,
    ) {
        let Some(settling) = self
            .run_ahead
            .settling
            .take_if(|settling| settling.round == *round && settling.fork.session == fork)
        else {
            return;
        };
        match result {
            Ok(agent) => self.settled(&settling, &agent),
            Err(failure) => self.not_settled(&settling, &failure),
        }
    }

    fn settled(&mut self, settling: &Settling, agent: &Agent) {
        let Settling {
            round,
            fork,
            target,
            reason,
            ..
        } = settling;
        let now = review_explore::now_ms();
        let saved = self.update_forks(round, |forks| {
            let Some(record) = forks.fork_mut(&fork.session) else {
                return;
            };
            record.continued = Some(match target {
                Target::Origin => Continuation::Undone {
                    at_ms: now,
                    reason: reason.clone(),
                },
                Target::Fork => Continuation::Switched { at_ms: now },
            });
            match target {
                Target::Origin => record.discard(DiscardReason::Answered, now),
                Target::Fork => forks.prepared_turn_used(),
            }
        });
        if let Err(error) = saved {
            self.run_ahead
                .host
                .log(&format!("the agent's session was not recorded: {error}"));
            self.run_ahead.left_unrecorded.insert(fork.session.clone());
        }
        match target {
            Target::Origin => {
                self.discard_in_background(round, fork.trace());
                self.run_ahead.host.log(&format!(
                    "the agent runs its own session again; the fork {} is discarded",
                    fork.session
                ));
            }
            Target::Fork => {
                if let Ok(forks) = self.rounds.forks(&round.unit, &round.instance)
                    && let Some(request) = forks.continued_as(&fork.session)
                {
                    let path = TurnPath::Prepared {
                        session: fork.session.clone(),
                    };
                    self.show_path(round, request, path);
                }
                self.run_ahead.host.log(&format!(
                    "the agent runs the session of the fork {}",
                    fork.session
                ));
            }
        }
        self.run_ahead.settle_failed.remove(&fork.session);
        self.follow_agent(round, agent);
        self.after_agent_moved();
    }

    fn not_settled(&mut self, settling: &Settling, failure: &SwitchFailure) {
        let Settling {
            round,
            fork,
            reason,
            trigger,
            ..
        } = settling;
        self.run_ahead.host.log(&format!(
            "the agent was not settled after the fork {}: {}",
            fork.session, failure.error
        ));
        if *trigger == Trigger::RunAhead && failure.typed {
            self.run_ahead.settle_failed.insert(fork.session.clone());
        }
        let now = review_explore::now_ms();
        let _ = self.update_forks(round, |forks| {
            if let Some(record) = forks.fork_mut(&fork.session)
                && matches!(record.continued, Some(Continuation::Switching { .. }))
            {
                record.continued = Some(Continuation::Failed {
                    at_ms: now,
                    error: format!("{reason}; {}", failure.error),
                    typed: true,
                });
            }
        });
        if let Some(held) = self.run_ahead.held.take()
            && self.still_waits(&held)
        {
            self.prompt_finished(
                ui_events::ExploreFinished {
                    instance: held.request.instance.clone(),
                    request: held.request.request.clone(),
                    result: Err(format!(
                        "The agent in the pane may still run the session of a turn prepared \
                         while you were thinking, and it could not be put back on its own \
                         session: {}. Look at the agent's pane, then Retry.",
                        failure.error
                    )),
                },
                &held.attempt,
            );
        }
    }

    pub(super) fn follow_agent(&mut self, round: &RoundKey, agent: &Agent) {
        if self
            .state
            .round
            .as_ref()
            .is_none_or(|saved| saved.exploration.instance != round.instance)
        {
            return;
        }
        self.state.agent = Some(PinnedAgent::new(agent.clone()));
        let followed = self.rounds.update(&round.unit, &round.instance, |saved| {
            saved.last_agent_session = ConversationBinding::from_agent(agent);
            Ok(())
        });
        match followed {
            Ok(((), saved)) => self.state.round = Some(saved),
            Err(error) => self
                .run_ahead
                .host
                .log(&format!("the agent's session was not recorded: {error}")),
        }
    }

    pub(super) fn after_agent_moved(&mut self) {
        if self.run_ahead.switching.is_some() || self.run_ahead.settling.is_some() {
            return;
        }
        if let Some(held) = self.run_ahead.held.take()
            && self.still_waits(&held)
            && !self.settle_before(&held)
        {
            self.send_turn(held);
        }
        if self
            .run_ahead
            .armed
            .as_ref()
            .is_some_and(|armed| armed.taken.is_none())
        {
            self.run_ahead_take();
        }
    }

    fn still_waits(&self, held: &crate::turn::SavedTurn) -> bool {
        self.state
            .is_pending(&held.request.instance, &held.request.request)
    }
}
