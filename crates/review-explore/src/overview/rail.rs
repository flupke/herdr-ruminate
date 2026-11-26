use serde::Serialize;
use ts_rs::TS;

use super::quiz::QuizStage;
use super::steps::{Activity, Current, Position};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, TS)]
pub struct RailStep {
    pub step: Step,
    pub state: StepState,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Step {
    Design,
    Question { number: usize },
    Quiz { stage: QuizStage },
    Conclusion,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StepState {
    Done,
    Current { working: bool },
    Later,
}

impl RailStep {
    pub(super) fn rail(design: bool, questions: usize, position: &Position<'_>) -> Vec<Self> {
        let step = |step: Step, at: Current, done: bool| Self {
            step,
            state: StepState::at(position, at, done),
        };
        let mut rail = Vec::new();
        if design {
            rail.push(step(Step::Design, Current::Design, true));
        }
        rail.extend(
            (1..=questions)
                .map(|number| step(Step::Question { number }, Current::Question(number), true)),
        );
        if let Some(quiz) = Step::quiz(position) {
            rail.push(step(quiz, Current::Quiz, position.concluded()));
        }
        rail.push(step(Step::Conclusion, Current::Conclusion, false));
        rail
    }
}

impl StepState {
    fn at(position: &Position<'_>, step: Current, done: bool) -> Self {
        if position.current == step {
            Self::Current {
                working: position.activity == Activity::Working,
            }
        } else if done {
            Self::Done
        } else {
            Self::Later
        }
    }
}

impl Step {
    fn quiz(position: &Position<'_>) -> Option<Self> {
        let stage = if position.concluded() {
            position.quiz()?.stage()
        } else {
            QuizStage::Later
        };
        Some(Self::Quiz { stage })
    }
}
