use serde::Serialize;
use ts_rs::TS;

use crate::QuizAnswers;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QuizStage {
    Later,
    Running {
        item: usize,
        items: usize,
    },
    Scored {
        correct: usize,
        answered: usize,
        items: usize,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct QuizProgress<'a> {
    pub items: usize,
    pub answers: Option<&'a QuizAnswers>,
}

impl QuizProgress<'_> {
    pub(super) fn running(&self) -> bool {
        self.next_item().is_some()
    }

    fn next_item(&self) -> Option<usize> {
        match self.answers {
            Some(answers) => answers.next_item(self.items),
            None => Some(0),
        }
    }

    pub(super) fn stage(&self) -> QuizStage {
        let items = self.items;
        let finished = self
            .answers
            .filter(|answers| answers.next_item(items).is_none());
        match finished {
            Some(answers) => QuizStage::Scored {
                correct: answers.correct_picks(),
                answered: answers.picks.len(),
                items,
            },
            None => QuizStage::Running {
                item: self.next_item().unwrap_or(0) + 1,
                items,
            },
        }
    }
}
