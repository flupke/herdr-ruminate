mod answer;
mod quiz;
mod rail;
mod record;
mod steps;
mod title;

pub use answer::{Decision, DecisionTag, KeptAnswer};
pub use quiz::{QuizProgress, QuizStage};
pub use rail::{RailStep, Step, StepState};
pub use record::{AgentRecord, CancelFrom, EarlierQuestion};
pub use title::TabTitle;

use serde::Serialize;

use crate::{Exploration, ExploreRound, Question, ReviewerAnswer};
pub use steps::{Activity, LatestTurn};
use steps::{Position, QuestionStep};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RoundOverview {
    pub rail: Vec<RailStep>,
    pub decisions: Vec<Decision>,
    pub earlier: Vec<EarlierQuestion>,
    pub title: TabTitle,
    pub glossary: std::collections::BTreeMap<String, String>,
}

impl RoundOverview {
    pub fn of(round: &ExploreRound, delivering: Option<&str>) -> Self {
        let exploration = &round.exploration;
        let steps = QuestionStep::of(exploration);
        let position = Position::of(exploration, &steps, delivering);
        Self {
            rail: RailStep::rail(
                exploration.design().is_some() || exploration.conversation.is_empty(),
                steps.len(),
                &position,
            ),
            decisions: Decision::of_round(exploration),
            earlier: steps
                .iter()
                .filter(|step| position.passed(step))
                .map(|step| EarlierQuestion::of(round, step))
                .collect(),
            title: TabTitle::of(&position),
            glossary: exploration.glossary.clone(),
        }
    }
}

impl RoundOverview {
    pub fn answers_earlier(&self, answer: &str) -> bool {
        self.earlier.iter().any(|earlier| {
            earlier
                .cancel
                .as_ref()
                .is_some_and(|cancel| cancel.answer == answer)
        })
    }
}

impl Exploration {
    pub fn question_number(&self, question: &Question) -> Option<usize> {
        QuestionStep::of(self)
            .iter()
            .rev()
            .find(|step| step.holds(question))
            .map(|step| step.number)
    }

    pub fn answered_number(&self, answer: &ReviewerAnswer) -> Option<usize> {
        self.question_number(answer.question.as_ref()?)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RoundStanding<'a> {
    pub design: bool,
    pub questions: usize,
    pub activity: Activity,
    pub latest: LatestTurn<'a>,
}

impl RoundStanding<'_> {
    pub fn overview(&self) -> RoundOverview {
        let position = Position::new(self.activity, self.latest, self.questions);
        RoundOverview {
            rail: RailStep::rail(self.design, self.questions, &position),
            decisions: Vec::new(),
            earlier: Vec::new(),
            title: TabTitle::of(&position),
            glossary: std::collections::BTreeMap::new(),
        }
    }
}
