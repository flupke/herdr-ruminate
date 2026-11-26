use review_explore::{KeptAnswer, MarkPhrase, MarkTense};
use serde::Serialize;
use ts_rs::TS;

use super::QuestionView;
use crate::round::RoundSnapshot;
use crate::status::StatusCard;

#[derive(Debug, Serialize, TS)]
pub(crate) struct SentView {
    card: StatusCard,
    number: Option<usize>,
    question: Option<QuestionView>,
    answer: KeptAnswer,
    marked: Option<MarkPhrase>,
    marks_problem: Option<String>,
}

impl SentView {
    pub(crate) fn of(round: &RoundSnapshot, offers_actions: bool) -> Option<Self> {
        let sent = round
            .stage
            .sent()
            .filter(|_| offers_actions || round.past)?;
        let number = round.sent_number();
        let marked = sent.marked.covered_phrase(MarkTense::Applied);
        Some(Self {
            card: StatusCard::of_turn(round, offers_actions)?,
            number,
            question: sent.question.as_ref().map(|answered| {
                QuestionView::answered(round.round.clone(), number.unwrap_or_default(), answered)
                    .keeping(sent.kept.choice.as_deref())
            }),
            answer: sent.kept.clone(),
            marked: (!marked.parts.is_empty()).then_some(marked),
            marks_problem: sent.marks_problem.clone(),
        })
    }
}
