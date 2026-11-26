use serde::Serialize;

use super::answer::KeptAnswer;
use super::steps::QuestionStep;
use crate::{ExploreRound, Interpretation, Question, TurnMarks};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EarlierQuestion {
    pub number: usize,
    pub question: Question,
    pub answer: Option<KeptAnswer>,
    pub recorded: AgentRecord,
    pub marks: Vec<TurnMarks>,
    pub cancel: Option<CancelFrom>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, ts_rs::TS)]
pub struct CancelFrom {
    pub answer: String,
    pub later: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct AgentRecord {
    pub interpretation: Option<Interpretation>,
    pub reply: Option<String>,
}

impl EarlierQuestion {
    pub(super) fn of(round: &ExploreRound, step: &QuestionStep<'_>) -> Self {
        let exploration = &round.exploration;
        let answer = step.answer(exploration);
        Self {
            number: step.number,
            question: step.question().clone(),
            answer: answer.map(KeptAnswer::of),
            recorded: answer.map_or_else(AgentRecord::default, |answer| AgentRecord {
                interpretation: exploration.interpretation(&answer.id).cloned(),
                reply: exploration
                    .turn_after(&answer.id)
                    .and_then(|turn| turn.update.reply.as_ref())
                    .map(|reply| reply.text.clone()),
            }),
            marks: round
                .marks_applied_by(|answer| step.answered_by(answer))
                .cloned()
                .collect(),
            cancel: answer.and_then(|answer| {
                let index = exploration
                    .answers
                    .iter()
                    .position(|kept| kept.id == answer.id)?;
                Some(CancelFrom {
                    answer: answer.id.clone(),
                    later: exploration.answers.len() - index - 1,
                })
            }),
        }
    }
}
