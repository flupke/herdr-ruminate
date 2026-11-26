use review_explore::{Alternative, TurnRequest};
use review_explore_runner::Unreviewed;
use review_run_ahead::{AnswerRecord, DiscardReason, Failure, ForkPoint, PlainReason, TurnPath};

use super::finishing::Finishing;
use super::switch::Switching;
use super::take::ForkFiles;
use super::{Armed, Asked, TakenFork};
use crate::ExploreSession;
use crate::turn::SavedTurn;

impl ExploreSession {
    pub(crate) fn with_reserved_ids(&self, mut request: TurnRequest) -> TurnRequest {
        if let Some(reserved) = self
            .run_ahead
            .armed
            .as_ref()
            .and_then(|armed| armed.reserved_for(&request))
        {
            reserved.apply(&mut request);
        }
        request
    }

    pub(crate) fn run_ahead_answer(&mut self, turn: &SavedTurn) -> bool {
        let answered_at_ms = review_explore::now_ms();
        let request = &turn.request;
        let Some(answer) = &request.answer else {
            return false;
        };
        let Some(armed) = &self.run_ahead.armed else {
            return false;
        };
        if !answer.question.as_ref().is_some_and(|question| {
            armed
                .asked
                .question
                .is_version(&question.id, question.version)
        }) {
            self.run_ahead_discard(DiscardReason::QuestionGone);
            return false;
        }
        let (asked, asked_at_ms) = (armed.asked.clone(), armed.asked_at_ms);
        let chosen = self.fork_for(armed, request);
        let taken = self
            .run_ahead
            .armed
            .as_mut()
            .and_then(|armed| armed.taken.take());
        let path = match (chosen, taken) {
            (Ok(index), Some(mut taken)) => {
                let fork = taken.forks.remove(index);
                self.discard_forks(&asked.round, &taken.forks, DiscardReason::Answered);
                if fork.kept.is_some() {
                    self.continue_as(Switching {
                        asked: asked.clone(),
                        turn: turn.clone(),
                        dispatch: self.dispatch_of(request, &turn.attempt, None),
                        fork,
                        files: taken.files,
                        hold: self.prompts.hold(),
                        finishing_since_ms: None,
                        stopped: false,
                    })
                } else if let Err(reason) = self.same_turn(&turn.request, &fork, &taken.files) {
                    self.discard_forks(
                        &asked.round,
                        std::slice::from_ref(&fork),
                        DiscardReason::Answered,
                    );
                    TurnPath::Plain { reason }
                } else {
                    self.wait_for_fork(Finishing {
                        asked: asked.clone(),
                        asked_at_ms,
                        turn: turn.clone(),
                        point: taken.point,
                        taken_at_ms: taken.at_ms,
                        fork,
                        files: taken.files,
                    })
                }
            }
            (chosen, taken) => {
                if let Some(taken) = &taken {
                    self.discard_forks(&asked.round, &taken.forks, DiscardReason::Answered);
                }
                TurnPath::Plain {
                    reason: chosen.err().unwrap_or(PlainReason::NoForks { why: None }),
                }
            }
        };
        let continues = self.record_answer(&asked, request, &path, answered_at_ms);
        self.run_ahead.armed = None;
        continues
    }

    pub(super) fn continue_as(&mut self, switching: Switching) -> TurnPath {
        if let Err(reason) =
            self.same_turn(&switching.turn.request, &switching.fork, &switching.files)
        {
            self.discard_switching_fork(&switching);
            return TurnPath::Plain { reason };
        }
        let session = switching.fork.session.clone();
        match self.start_switch(switching) {
            Ok(()) => TurnPath::Prepared { session },
            Err(reason) => TurnPath::Plain { reason },
        }
    }

    fn fork_for(&self, armed: &Armed, request: &TurnRequest) -> Result<usize, PlainReason> {
        let choice = self.forked_choice(armed, request)?;
        let taken = armed.taken.as_ref().ok_or_else(|| PlainReason::NoForks {
            why: armed.refusal.clone(),
        })?;
        let index = taken
            .forks
            .iter()
            .position(|fork| fork.choice == choice.id)
            .ok_or(PlainReason::NotForked)?;
        let fork = &taken.forks[index];
        if fork.kept.is_none() && fork.ended {
            return Err(PlainReason::NoTurn);
        }
        self.unchanged_since_forks(&armed.asked, armed.asked_at_ms, &taken.point, taken.at_ms)?;
        Ok(index)
    }

    pub(super) fn unchanged_since_forks(
        &self,
        asked: &Asked,
        asked_at_ms: u64,
        point: &ForkPoint,
        taken_at_ms: u64,
    ) -> Result<(), PlainReason> {
        let unchecked = |error: &dyn std::fmt::Display| PlainReason::Unchecked {
            error: error.to_string(),
        };
        if self
            .rounds
            .reviewer_wrote_unknown_to_forks(
                &asked.round.unit,
                &asked.round.instance,
                &asked.question.id,
                asked_at_ms,
                taken_at_ms,
            )
            .map_err(|error| unchecked(&error))?
        {
            return Err(PlainReason::ChatMessage);
        }
        let agent = self
            .agents
            .get_agent(&asked.pane)
            .map_err(|error| unchecked(&error))?
            .ok_or_else(|| unchecked(&"the agent's pane is gone"))?;
        if !agent.agent_status.waits_for_prompt() {
            return Err(PlainReason::AgentBusy);
        }
        let session = self.run_ahead.host.session(&agent);
        if session.as_ref() != Some(&point.session)
            || self.run_ahead.host.last_entry(point) != point.entry
        {
            return Err(PlainReason::SessionMoved);
        }
        Ok(())
    }

    fn forked_choice<'a>(
        &self,
        armed: &Armed,
        request: &'a TurnRequest,
    ) -> Result<&'a Alternative, PlainReason> {
        let answer = request.answer.as_ref().expect("an answer");
        if self
            .agents
            .get_agent(&armed.asked.pane)
            .ok()
            .flatten()
            .is_some_and(|agent| {
                self.run_ahead.host.unhooked(&agent).is_some()
                    && self.run_ahead.host.unforkable(&agent).is_none()
            })
        {
            return Err(PlainReason::NoHooks);
        }
        if answer.drawing_board {
            return Err(PlainReason::DrawingBoard);
        }
        let choice = answer
            .option
            .as_ref()
            .filter(|choice| !choice.is_none_of_the_above())
            .ok_or(PlainReason::NoneOfTheAbove)?;
        if !answer.text.trim().is_empty() {
            return Err(PlainReason::Comment);
        }
        if armed.talk.is_some() {
            return Err(PlainReason::ChatMessage);
        }
        Ok(choice)
    }

    fn same_turn(
        &mut self,
        request: &TurnRequest,
        fork: &TakenFork,
        files: &ForkFiles,
    ) -> Result<(), PlainReason> {
        let unchecked = |error: eyre::Report| PlainReason::Unchecked {
            error: format!("{error:#}"),
        };
        let comparison = self.turn_comparison(request).map_err(unchecked)?;
        let unreviewed = self.turn_unreviewed().map_err(unchecked)?;
        if !self
            .diffs
            .as_ref()
            .is_some_and(|diffs| diffs.same_lines(files.diffs()))
        {
            return Err(PlainReason::UnreviewedChanged);
        }
        let unreviewed = Unreviewed {
            directory: files.diffs().directory().to_owned(),
            index: files.diffs().index(),
            ..unreviewed
        };
        let turn = self
            .turn_instructions(request, &comparison, &fork.access, &unreviewed)
            .map_err(unchecked)?;
        if turn.files() != fork.turn.files() {
            return Err(PlainReason::PromptChanged);
        }
        Ok(())
    }

    fn record_answer(
        &mut self,
        asked: &Asked,
        request: &TurnRequest,
        path: &TurnPath,
        at_ms: u64,
    ) -> bool {
        let answer = request.answer.as_ref().expect("an answer");
        let record = AnswerRecord {
            question: asked.question.id.clone(),
            version: asked.question.version,
            answer: answer.id.clone(),
            request: request.request.clone(),
            at_ms,
            path: path.clone(),
        };
        let line = match path {
            TurnPath::Prepared { session } => {
                format!("the answer continues as the fork {session}")
            }
            TurnPath::Plain { reason } => {
                format!("the answer goes to the agent in the pane: {reason:?}")
            }
        };
        self.run_ahead.log(&line);
        if let Err(error) = self.update_forks(&asked.round, |forks| forks.answers.push(record)) {
            self.run_ahead
                .log(&format!("the answer's path was not recorded: {error}"));
        }
        let reason = match path {
            TurnPath::Prepared { .. } => return true,
            TurnPath::Plain { reason } => reason,
        };
        self.show_path(&asked.round, &request.request, path.clone());
        if let Some(failure) = Failure::of_plain(reason) {
            self.run_ahead_failed(&asked.round, failure);
        }
        false
    }
}
