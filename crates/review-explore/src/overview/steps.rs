use super::quiz::QuizProgress;
use crate::{Exploration, Question, ReviewerAnswer};

pub(super) struct QuestionStep<'a> {
    pub(super) number: usize,
    versions: &'a [Question],
}

impl<'a> QuestionStep<'a> {
    pub(super) fn of(exploration: &'a Exploration) -> Vec<Self> {
        exploration
            .questions
            .chunk_by(|earlier, later| earlier.id == later.id)
            .enumerate()
            .map(|(index, versions)| Self {
                number: index + 1,
                versions,
            })
            .collect()
    }

    pub(super) fn question(&self) -> &'a Question {
        self.versions.last().expect("a step has a version")
    }

    pub(super) fn answer(&self, exploration: &'a Exploration) -> Option<&'a ReviewerAnswer> {
        let question = self.question();
        exploration
            .answers
            .iter()
            .rev()
            .find(|answer| answer.answers(question))
    }

    pub(super) fn holds(&self, question: &Question) -> bool {
        self.versions
            .iter()
            .any(|version| version.is_version(&question.id, question.version))
    }

    pub(super) fn answered_by(&self, answer: &ReviewerAnswer) -> bool {
        self.versions
            .iter()
            .any(|question| answer.answers(question))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Activity {
    Working,
    Interrupted,
    Idle,
}

#[derive(Clone, Copy, Debug)]
pub enum LatestTurn<'a> {
    None,
    Question,
    Conclusion { quiz: Option<QuizProgress<'a>> },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Current {
    Design,
    Question(usize),
    Quiz,
    Conclusion,
}

pub(super) struct Position<'a> {
    pub(super) activity: Activity,
    latest: LatestTurn<'a>,
    pub(super) current: Current,
}

impl<'a> Position<'a> {
    pub(super) fn of(
        exploration: &'a Exploration,
        steps: &[QuestionStep<'_>],
        delivering: Option<&str>,
    ) -> Self {
        Self::new(
            Self::activity(exploration, delivering),
            Self::latest(exploration),
            steps.len(),
        )
    }

    pub(super) fn new(activity: Activity, latest: LatestTurn<'a>, questions: usize) -> Self {
        let current = match latest {
            LatestTurn::None => Current::Design,
            LatestTurn::Question => Current::Question(questions),
            LatestTurn::Conclusion { quiz: Some(quiz) } if quiz.running() => Current::Quiz,
            LatestTurn::Conclusion { .. } => Current::Conclusion,
        };
        Self {
            activity,
            latest,
            current,
        }
    }

    fn activity(exploration: &Exploration, delivering: Option<&str>) -> Activity {
        match exploration.pending_request() {
            Some(pending) if delivering == Some(pending.request.as_str()) => Activity::Working,
            Some(_) => Activity::Interrupted,
            None if exploration.retry_request().is_some() => Activity::Interrupted,
            None => Activity::Idle,
        }
    }

    fn latest(exploration: &'a Exploration) -> LatestTurn<'a> {
        let Some(turn) = exploration.conversation.last() else {
            return LatestTurn::None;
        };
        match &turn.update.conclusion {
            Some(conclusion) => LatestTurn::Conclusion {
                quiz: (!conclusion.quiz.is_empty()).then(|| QuizProgress {
                    items: conclusion.quiz.len(),
                    answers: exploration.quiz_answers(&turn.update.request),
                }),
            },
            None => LatestTurn::Question,
        }
    }

    pub(super) fn concluded(&self) -> bool {
        matches!(self.latest, LatestTurn::Conclusion { .. })
    }

    pub(super) fn quiz(&self) -> Option<QuizProgress<'a>> {
        match self.latest {
            LatestTurn::Conclusion { quiz } => quiz,
            _ => None,
        }
    }

    pub(super) fn passed(&self, step: &QuestionStep<'_>) -> bool {
        self.current != Current::Question(step.number)
    }
}
