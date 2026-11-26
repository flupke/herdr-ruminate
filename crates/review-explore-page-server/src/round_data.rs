use change_references::{ChangeReferences, PullRequest, Ticket};
use review_explore::{Conclusion, Design, Question, QuizAnswers, QuizItem};
use review_explore_page::{
    PageImplementation, PageQuiz, QuestionMarks, ReviewName, RoundStage, TurnResponse,
};

use crate::changed_source::FixedChange;
use crate::rich::Rich;
use crate::sessions::{RunAheadPath, Step};
use crate::short::Short;
use crate::tally::JevMark;

const CONCLUSION_REQUEST: &str = "conclusion";

pub(crate) trait RoundData: Send + Sync {
    fn review(&self) -> ReviewName;

    fn design(&self) -> Design;

    fn question(&self, number: usize) -> (Question, QuestionMarks);

    fn answer_response(&self, number: usize) -> TurnResponse;

    fn glossary(&self) -> std::collections::BTreeMap<String, String> {
        std::collections::BTreeMap::new()
    }

    fn pane_comment(&self) -> &'static str {
        ""
    }

    fn chat_reply(&self) -> &'static str {
        "The draft lives in the round: a reset drops it with the round, so `Keep the draft` costs \
         nothing once the round ends."
    }

    fn conclusion(&self, quiz: bool) -> Conclusion;

    fn conclusion_response(&self) -> TurnResponse;

    fn change(&self) -> &FixedChange;

    fn jev_marks(&self) -> &'static [JevMark];

    fn saved_rounds(&self) -> &'static [SavedRound] {
        SAVED_ROUNDS
    }

    fn question_stage(&self, number: usize, question: Option<Question>) -> RoundStage {
        let (question, marks) = match question {
            Some(question) => (question, QuestionMarks::default()),
            None => self.question(number),
        };
        let change = self.change();
        let citations = question
            .evidence
            .iter()
            .map(|evidence| change.cite(evidence))
            .collect();
        RoundStage::Question {
            number,
            question: Box::new(question),
            citations,
            marks,
            response: if number > 1 {
                self.answer_response(number)
            } else {
                TurnResponse::default()
            },
            answer_cancelled: false,
        }
    }

    fn conclusion_stage(
        &self,
        implementation: Option<PageImplementation>,
        quiz: Option<QuizAnswers>,
    ) -> RoundStage {
        let conclusion = self.conclusion(quiz.is_some());
        let change = self.change();
        let quiz = quiz
            .map(|answers| PageQuiz {
                proofs: conclusion
                    .quiz
                    .iter()
                    .map(|item| item.proof.iter().map(|proof| change.cite(proof)).collect())
                    .collect(),
                answers,
                takes_answers: true,
            })
            .unwrap_or_default();
        RoundStage::Conclusion {
            request: CONCLUSION_REQUEST.into(),
            conclusion: Box::new(conclusion),
            implementation,
            quiz,
            response: self.conclusion_response(),
        }
    }

    fn quiz_items(&self) -> Vec<QuizItem> {
        self.conclusion(true).quiz
    }

    fn to_be_implemented(&self) -> String {
        self.conclusion(false).to_be_implemented
    }
}

pub(crate) struct SavedRound {
    pub(crate) title: &'static str,
    pub(crate) revision: (&'static str, &'static str),
    pub(crate) minutes_ago: u64,
    pub(crate) steps: &'static [Step],
    pub(crate) pull_request: Option<u64>,
    pub(crate) tickets: &'static [&'static str],
}

impl SavedRound {
    pub(crate) fn references(&self) -> ChangeReferences {
        let github = "https://github.com/flupke/drafts-demo";
        ChangeReferences {
            pull_request: self.pull_request.map(|number| PullRequest {
                number,
                title: self.title.to_owned(),
                url: format!("{github}/pull/{number}"),
            }),
            tickets: self
                .tickets
                .iter()
                .map(|id| Ticket {
                    id: (*id).to_owned(),
                    url: id.strip_prefix('#').map_or_else(
                        || format!("https://linear.app/drafts/issue/{id}"),
                        |number| format!("{github}/issues/{number}"),
                    ),
                    title: None,
                })
                .collect(),
        }
    }
}

pub(crate) const MINUTE: u64 = 1;
pub(crate) const DAY: u64 = 24 * 60 * MINUTE;

const QUESTION: Step = Step::Question {
    path: RunAheadPath::Unwatched,
};
const ANSWER: Step = Step::Answer {
    after_first_pick: false,
};

pub(crate) const AT_FIRST_QUESTION: &[Step] = &[Step::Kickoff, QUESTION];
pub(crate) const AT_SECOND_QUESTION: &[Step] = &[Step::Kickoff, QUESTION, ANSWER, QUESTION];
pub(crate) const CONCLUDED: &[Step] = &[
    Step::Kickoff,
    QUESTION,
    ANSWER,
    QUESTION,
    ANSWER,
    Step::Conclude { quiz: true },
];

const SAVED_ROUNDS: &[SavedRound] = &[
    SavedRound {
        title: "Keep the reviewer's draft across reloads",
        revision: ("kq", "vmtsxy"),
        minutes_ago: DAY,
        steps: CONCLUDED,
        pull_request: None,
        tickets: &[],
    },
    SavedRound {
        title: "Name the queue's flush policy",
        revision: ("pw", "nlzrot"),
        minutes_ago: 3 * DAY,
        steps: AT_SECOND_QUESTION,
        pull_request: None,
        tickets: &[],
    },
];

pub(crate) fn by_name(name: &str) -> Option<&'static dyn RoundData> {
    match name {
        "short" => Some(&Short),
        "rich" => Some(&Rich),
        _ => None,
    }
}
