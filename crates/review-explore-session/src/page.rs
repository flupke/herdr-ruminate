use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use linear_issues::LinearIssue;
use review_explore::{
    CitedLines, CodeLocation, Comparison, DispatchState, EvidenceRef, Exploration, ExploreRound,
    ImplementationDelivery, KeptAnswer, Question, ReviewerAnswer, RoundOverview, TurnMarks,
    TurnRequest, Uncitable,
};
use review_explore_citations::{Citation, CodeColors};
use review_explore_page::{
    Answered, AnsweredQuestion, CommandRefusal, CommandReply, ImplementationState, Interruption,
    LatestAnswer, PageAnswer, PageCommand, PageImplement, PageImplementation, PageQuiz,
    PublishedRound, QuestionMarks, Recovery, ReviewName, RoundStage, SentAnswer, TurnResponse,
};
use review_repository::repository::{Repository, SnapshotIdentity};
use review_run_ahead::TurnPath;
use review_source::ReviewCheckpoint;

use crate::{ExploreSession, Start};

#[derive(Default)]
pub(crate) struct PageCitations {
    colors: CodeColors,
    shown: Option<Shown>,
}

struct Shown {
    checkpoint: ReviewCheckpoint,
    lists: Vec<Vec<EvidenceRef>>,
    citations: Vec<Arc<[Citation]>>,
}

#[derive(Clone, Copy)]
pub(crate) enum CitedCode<'a> {
    WorkingCopy(&'a Path),
    Saved(&'a Repository),
}

impl CitedCode<'_> {
    fn lines(
        self,
        comparison: &Comparison,
        location: &CodeLocation,
    ) -> Result<CitedLines, Uncitable> {
        match self {
            Self::WorkingCopy(root) => comparison.tracked_cited_lines(location, root),
            Self::Saved(repository) => comparison.saved_cited_lines(location, repository),
        }
    }
}

impl PageCitations {
    fn of(
        &mut self,
        question: &Question,
        comparison: &Comparison,
        code: CitedCode<'_>,
    ) -> Arc<[Citation]> {
        self.lists(std::slice::from_ref(&question.evidence), comparison, code)
            .swap_remove(0)
    }

    pub(crate) fn lists(
        &mut self,
        lists: &[Vec<EvidenceRef>],
        comparison: &Comparison,
        code: CitedCode<'_>,
    ) -> Vec<Arc<[Citation]>> {
        if let Some(shown) = &self.shown
            && shown.checkpoint == comparison.checkpoint
            && shown.lists == lists
        {
            return shown.citations.clone();
        }
        let citations: Vec<Arc<[Citation]>> = lists
            .iter()
            .map(|list| {
                list.iter()
                    .map(|evidence| {
                        let lines = code.lines(comparison, &evidence.location);
                        self.colors.cite(evidence.clone(), lines)
                    })
                    .collect()
            })
            .collect();
        self.shown = Some(Shown {
            checkpoint: comparison.checkpoint.clone(),
            lists: lists.to_vec(),
            citations: citations.clone(),
        });
        citations
    }
}

impl ExploreSession {
    pub(crate) fn publish_page(&mut self) {
        let stage = self.page_stage();
        let cancellable = self.cancellable();
        let overview = self.state.round.as_ref().map(|round| {
            let delivering = self
                .state
                .pending
                .as_ref()
                .filter(|(instance, _)| *instance == round.exploration.instance)
                .map(|(_, request)| request.as_str());
            RoundOverview::of(round, delivering)
        });
        let earlier_citations = self
            .state
            .round
            .as_ref()
            .zip(overview.as_ref())
            .map_or_else(Vec::new, |(round, overview)| {
                let lists: Vec<Vec<EvidenceRef>> = overview
                    .earlier
                    .iter()
                    .map(|record| record.question.evidence.clone())
                    .collect();
                self.earlier_citations.lists(
                    &lists,
                    &round.exploration.comparison,
                    CitedCode::WorkingCopy(self.repository.root()),
                )
            });
        let round = self
            .state
            .round
            .as_ref()
            .zip(overview.as_ref())
            .map(|(round, overview)| PublishedRound {
                id: &round.exploration.instance,
                review_unit: &round.exploration.comparison.checkpoint.review_unit,
                design: round.exploration.design(),
                changed_files: round.exploration.comparison.files.len(),
                author: round.exploration.author,
                cancellable: cancellable.as_ref(),
                earlier: self.state.historical,
                overview,
                earlier_citations: &earlier_citations,
            });
        self.page.publish_counted(round, stage, self.mark_tally());
    }

    pub(crate) fn cancellable(&self) -> Option<LatestAnswer> {
        let round = self.state.round.as_ref()?;
        if self.state.historical
            || self.state.storage_error.is_some()
            || !round.implementations.is_empty()
        {
            return None;
        }
        let answer = round.exploration.answers.last()?;
        Some(LatestAnswer {
            id: answer.id.clone(),
            choice: KeptAnswer::of(answer).choice,
            comment: answer.text.clone(),
            answered: answered(answer),
        })
    }

    pub fn name_review(&self, identity: &SnapshotIdentity, issues: Vec<LinearIssue>) {
        self.page.name(ReviewName {
            issues,
            ..ReviewName::of(self.repository.root(), identity)
        });
    }

    pub(crate) fn page_command(&mut self, command: PageCommand, reply: CommandReply) {
        match command {
            PageCommand::Answer(answer) => {
                let result = self.answer_from_page(answer);
                self.publish_page();
                reply.send(result);
            }
            PageCommand::Pick { question, version } => {
                let result = self.pick_from_page(&question, version);
                reply.send(result);
            }
            PageCommand::DiagramFailed(error) => {
                let result = self.diagram_failed(error);
                self.publish_page();
                reply.send(result);
            }
            PageCommand::Start { choices, start } => {
                if let Some(kickoff) = self.start_from_page(choices, &start, reply) {
                    let _ = self.deliver_turn(kickoff, None);
                }
            }
            PageCommand::Implement(implement) => {
                let result = self.implement_from_page(implement);
                self.publish_page();
                reply.send(result);
            }
            PageCommand::Quiz(response) => {
                let result = self.quiz_from_page(response);
                self.publish_page();
                reply.send(result);
            }
            PageCommand::Recover(recovery) => {
                self.recover_from_page(recovery, reply);
            }
        }
    }

    pub fn recover_from_page(&mut self, recovery: Recovery, reply: CommandReply) -> bool {
        let result = match recovery {
            Recovery::Stop(waiting) => self.stop(&waiting),
            Recovery::Reset { round } => self.reset_round(&round),
            Recovery::Retry { request, attempt } => self.retry_from_page(&request, &attempt),
            Recovery::CancelAnswer { answer } => self.cancel_answer_from_page(answer),
            Recovery::CancelImplementation { delivery } => {
                self.cancel_implementation_from_page(&delivery)
            }
            Recovery::ResendImplementation {
                conclusion,
                delivery,
                attempt,
            } => self.resend_from_page(&conclusion, &delivery, &attempt),
        };
        self.reply_to_page(reply, result)
    }

    fn reply_to_page(&mut self, reply: CommandReply, result: Result<(), CommandRefusal>) -> bool {
        self.publish_page();
        let done = result.is_ok();
        reply.send(result);
        done
    }

    pub(crate) fn saved_round(&self) -> Result<ExploreRound, CommandRefusal> {
        let shown = self.state.round.as_ref().ok_or(CommandRefusal::Stale)?;
        self.rounds
            .round(
                &shown.exploration.comparison.checkpoint.review_unit,
                &shown.exploration.instance,
            )
            .map_err(|error| CommandRefusal::Failed(error.to_string()))?
            .ok_or(CommandRefusal::Stale)
    }

    fn answer_from_page(&mut self, answer: PageAnswer) -> Result<(), CommandRefusal> {
        let failed = |error: &dyn std::fmt::Display| CommandRefusal::Failed(error.to_string());
        let round = self.saved_round()?;
        let Some(question) = waiting_question(&round.exploration)
            .filter(|question| question.is_version(&answer.question, answer.version))
            .cloned()
        else {
            let repeat = round.exploration.answers.last().is_some_and(|latest| {
                let latest_answered = answered(latest);
                latest_answered.question == Some((answer.question.clone(), answer.version))
                    && latest_answered.option == answer.input.option
                    && latest.text == answer.input.text
            });
            return Err(CommandRefusal::stale_unless_repeat(repeat));
        };
        let request = round
            .exploration
            .clone()
            .request(Some(answer.input), Some(&question))
            .map_err(|error| failed(&error))?;
        self.deliver_turn(request, None)
            .map_err(CommandRefusal::Failed)
    }

    fn pick_from_page(&self, question: &str, version: u32) -> Result<(), CommandRefusal> {
        let round = self.saved_round()?;
        waiting_question(&round.exploration)
            .filter(|waiting| waiting.is_version(question, version))
            .map(|_| ())
            .ok_or(CommandRefusal::Stale)
    }

    fn implement_from_page(&mut self, implement: PageImplement) -> Result<(), CommandRefusal> {
        let round = self.saved_round()?;
        if round.exploration.conclusion_request() != Some(implement.conclusion.as_str()) {
            return Err(CommandRefusal::Stale);
        }
        let sending = self.state.implementation.is_some();
        let offered = match round.latest_implementation(&implement.conclusion) {
            None => implement.replaces.is_none(),
            Some(latest) => {
                page_implementation(latest, sending).state.allows_another()
                    && implement.replaces.as_deref() == Some(latest.request.delivery.as_str())
            }
        };
        if !offered {
            let repeat = round
                .latest_implementation(&implement.conclusion)
                .is_some_and(|latest| {
                    latest.request.text == implement.text
                        && !page_implementation(latest, sending).state.allows_another()
                        && implement.replaces.as_deref() != Some(latest.request.delivery.as_str())
                });
            return Err(CommandRefusal::stale_unless_repeat(repeat));
        }
        let request = round
            .exploration
            .implementation(implement.text)
            .map_err(|error| CommandRefusal::Failed(error.to_string()))?;
        self.implement(request).map_err(CommandRefusal::Failed)
    }

    fn page_stage(&mut self) -> RoundStage {
        if let Some(failure) = &self.state.storage_error {
            return RoundStage::StorageFailed {
                failure: failure.clone(),
            };
        }
        let Some(round) = &self.state.round else {
            return match &self.state.start {
                Start::Idle { offer } => RoundStage::NoRound {
                    start: offer.clone(),
                },
                Start::Starting {
                    start,
                    started_at_ms,
                } => RoundStage::Starting {
                    start: start.clone(),
                    started_at_ms: Some(*started_at_ms),
                },
                Start::Failed { failure, offer } => RoundStage::StartFailed {
                    failure: failure.clone(),
                    start: offer.clone(),
                },
            };
        };
        let exploration = &round.exploration;
        let code = CitedCode::WorkingCopy(self.repository.root());
        if let Some(request) = exploration.pending_request() {
            let delivering = self.state.pending.as_ref().is_some_and(|(instance, id)| {
                *instance == request.instance && *id == request.request
            });
            let answer = sent_answer(round, request, &mut self.citations, code);
            return if delivering {
                RoundStage::AgentWorking {
                    request: request.request.clone(),
                    sent_at_ms: round
                        .turns
                        .get(&request.request)
                        .and_then(|delivery| delivery.started_at_ms),
                    finishing_since_ms: self.run_ahead_finishing_since(&request.request),
                    not_prepared: match self.run_ahead.turn_paths().get(&request.request) {
                        Some(path @ TurnPath::Plain { .. }) => path.line().map(Into::into),
                        _ => None,
                    },
                    answer,
                }
            } else {
                interrupted(round, request, answer)
            };
        }
        if let Some(retry) = exploration.retry_request() {
            let answer = sent_answer(round, retry, &mut self.citations, code);
            return interrupted(round, retry, answer);
        }
        let sending = self.state.implementation.is_some();
        let takes_quiz_answers = !self.state.historical && self.state.storage_error.is_none();
        latest_turn(
            round,
            LatestTurn {
                sending,
                takes_quiz_answers,
                paths: self.run_ahead.turn_paths(),
            },
            &mut self.citations,
            code,
        )
    }
}

pub(crate) fn sent_answer(
    round: &ExploreRound,
    request: &TurnRequest,
    citations: &mut PageCitations,
    code: CitedCode<'_>,
) -> Option<Box<SentAnswer>> {
    let answer = request.answer.as_ref()?;
    Some(Box::new(SentAnswer {
        question: answer.question.as_ref().map(|question| AnsweredQuestion {
            citations: citations.of(question, &round.exploration.comparison, code),
            question: Box::new(question.clone()),
            picked_blind: answer.first_pick.is_some(),
        }),
        kept: KeptAnswer::of(answer),
        marked: answer_marks(round, &answer.in_reply_to, &answer.id)
            .map(TurnMarks::counts)
            .unwrap_or_default(),
        marks_problem: answer_marks(round, &answer.in_reply_to, &answer.id)
            .and_then(|marks| marks.problem.clone()),
    }))
}

fn answer_marks<'a>(round: &'a ExploreRound, request: &str, answer: &str) -> Option<&'a TurnMarks> {
    round
        .marks
        .get(request)
        .filter(|marks| marks.answer.as_deref() == Some(answer))
}

pub(crate) fn interrupted(
    round: &ExploreRound,
    request: &TurnRequest,
    answer: Option<Box<SentAnswer>>,
) -> RoundStage {
    let delivery = round.turns.get(&request.request);
    let state = delivery.map(|delivery| &delivery.state);
    let interruption = match (state, &request.response_error) {
        (Some(DispatchState::NotStarted), _) => Interruption::NotStarted,
        (_, Some(failure)) => Interruption::Failed(failure.clone()),
        (Some(DispatchState::Attempting | DispatchState::Unknown), None) => Interruption::Uncertain,
        (_, None) => Interruption::Stopped,
    };
    RoundStage::Interrupted {
        request: Some(request.request.clone()),
        attempt: delivery.map(|delivery| delivery.attempt.clone()),
        interruption,
        answer,
    }
}

fn waiting_question(exploration: &Exploration) -> Option<&Question> {
    exploration.waiting_turn()?.next.as_ref()
}

#[derive(Clone, Copy)]
pub(crate) struct LatestTurn<'a> {
    pub(crate) sending: bool,
    pub(crate) takes_quiz_answers: bool,
    pub(crate) paths: &'a BTreeMap<String, TurnPath>,
}

pub(crate) fn latest_turn(
    round: &ExploreRound,
    latest: LatestTurn<'_>,
    citations: &mut PageCitations,
    code: CitedCode<'_>,
) -> RoundStage {
    let exploration = &round.exploration;
    let nothing = || RoundStage::Interrupted {
        request: None,
        attempt: None,
        interruption: Interruption::Stopped,
        answer: None,
    };
    let Some(turn) = exploration.conversation.last() else {
        return nothing();
    };
    let answered = match turn.update.conclusion {
        Some(_) => round.marks.get(&turn.update.request),
        None => turn.answer.as_deref().and_then(|answer| {
            let answer = exploration.answers.iter().find(|kept| kept.id == answer)?;
            answer_marks(round, &answer.in_reply_to, &answer.id)
        }),
    };
    let response = TurnResponse {
        path: latest.paths.get(&turn.update.request).cloned(),
        marks_problem: answered.and_then(|marks| marks.problem.clone()),
        ..TurnResponse::of(exploration, turn)
    };
    if let Some(conclusion) = &turn.update.conclusion {
        let request = &turn.update.request;
        let proofs: Vec<Vec<EvidenceRef>> = conclusion
            .quiz
            .iter()
            .map(|item| item.proof.clone())
            .collect();
        return RoundStage::Conclusion {
            request: request.clone(),
            conclusion: Box::new(conclusion.clone()),
            implementation: round
                .latest_implementation(request)
                .map(|delivery| page_implementation(delivery, latest.sending)),
            quiz: PageQuiz {
                proofs: citations.lists(&proofs, &exploration.comparison, code),
                answers: exploration
                    .quiz_answers(request)
                    .cloned()
                    .unwrap_or_default(),
                takes_answers: latest.takes_quiz_answers,
            },
            response,
        };
    }
    match &turn.update.next {
        Some(question) => RoundStage::Question {
            number: exploration.questions.len(),
            question: Box::new(question.clone()),
            citations: citations.of(question, &exploration.comparison, code),
            marks: QuestionMarks::requested(&turn.update),
            response,
            answer_cancelled: exploration.cancelled_since_last_turn(),
        },
        None => nothing(),
    }
}

fn page_implementation(delivery: &ImplementationDelivery, sending: bool) -> PageImplementation {
    let state = ImplementationState::of(&delivery.state, sending);
    PageImplementation {
        delivery: delivery.request.delivery.clone(),
        attempt: delivery.attempt.clone(),
        text: delivery.request.text.clone(),
        state,
        sent_at_ms: delivery.sent_at_ms,
    }
}

pub(crate) fn answered(answer: &ReviewerAnswer) -> Answered {
    Answered {
        question: answer
            .question
            .as_ref()
            .map(|question| (question.id.clone(), question.version)),
        option: answer.option.as_ref().map(|option| option.id.clone()),
        in_reply_to: answer.in_reply_to.clone(),
        drawing_board: answer.sends_to_drawing_board(),
    }
}
