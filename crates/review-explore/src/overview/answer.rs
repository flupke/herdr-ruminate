use serde::Serialize;
use ts_rs::TS;

use super::steps::QuestionStep;
use crate::{Alternative, Exploration, ReviewerAnswer};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, TS)]
pub struct KeptAnswer {
    pub choice: Option<String>,
    pub comment: String,
    pub tags: Vec<DecisionTag>,
    pub drawing_board: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum DecisionTag {
    ChangedAfterFirstPick,
    AsRecommended,
}

const DRAWING_BOARD: &str = "Back to the drawing board";

impl KeptAnswer {
    pub fn of(answer: &ReviewerAnswer) -> Self {
        let kept = Self::new(
            answer.option.as_ref(),
            &answer.text,
            answer.first_pick.as_deref(),
        );
        if answer.drawing_board {
            return kept.into_drawing_board();
        }
        kept
    }

    #[must_use]
    pub fn into_drawing_board(self) -> Self {
        Self {
            choice: Some(DRAWING_BOARD.to_owned()),
            tags: Vec::new(),
            drawing_board: true,
            ..self
        }
    }

    pub fn new(kept: Option<&Alternative>, comment: &str, first_pick: Option<&str>) -> Self {
        let changed = first_pick.is_some_and(|first| kept.is_none_or(|kept| kept.id != first));
        let recommended = kept.is_some_and(|kept| kept.recommendation.is_some());
        Self {
            choice: kept.map(|kept| kept.text.clone()),
            comment: comment.to_owned(),
            tags: [
                changed.then_some(DecisionTag::ChangedAfterFirstPick),
                recommended.then_some(DecisionTag::AsRecommended),
            ]
            .into_iter()
            .flatten()
            .collect(),
            drawing_board: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Decision {
    pub number: usize,
    pub question: String,
    pub answer: KeptAnswer,
}

impl Decision {
    pub fn of_round(exploration: &Exploration) -> Vec<Self> {
        QuestionStep::of(exploration)
            .iter()
            .filter_map(|step| Self::of(exploration, step))
            .collect()
    }

    pub(super) fn of(exploration: &Exploration, step: &QuestionStep<'_>) -> Option<Self> {
        let answer = step.answer(exploration)?;
        Some(Self {
            number: step.number,
            question: step.question().text.clone(),
            answer: KeptAnswer::of(answer),
        })
    }
}
