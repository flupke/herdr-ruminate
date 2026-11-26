use crate::{
    AgendaChange, Assessments, CodeLocation, Comparison, ConversationTurn, EvidenceRef, Reply,
};
use review_explore_round_settings::WritingStyle;
use review_source::ReviewCheckpoint;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};

#[derive(
    Clone, Copy, Debug, Default, Deserialize, Serialize, Eq, PartialEq, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum TopicStatus {
    #[default]
    Open,
    Accepted,
    NeedsFollowUp,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeAuthor {
    #[default]
    Reviewer,
    Other,
}

impl ChangeAuthor {
    pub fn is_reviewer(&self) -> bool {
        *self == Self::Reviewer
    }

    pub fn toggle(&mut self) {
        *self = match self {
            Self::Reviewer => Self::Other,
            Self::Other => Self::Reviewer,
        };
    }
}

pub const ASK_AUTHOR: &str = "ask-author";

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Topic {
    pub id: String,
    pub title: String,
    pub entries: Vec<CodeLocation>,
    /// The reviewer's decision on the topic, which only the interpretation of an answer
    /// changes. The tools leave it out: a topic the agent sends keeps its recorded status.
    #[serde(default)]
    #[schemars(skip)]
    pub status: TopicStatus,
    /// Provisional inquiry; posted questions retain their own immutable wording.
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub prerequisites: Vec<String>,
    /// The topic's place in the agenda: a lower rank comes first.
    #[serde(default)]
    pub rank: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Alternative {
    pub id: String,
    /// The choice as the reviewer reads it. It never says that the choice is recommended,
    /// preferred or the default: that goes only in `recommendation`.
    pub text: String,
    pub outcome: TopicStatus,
    /// Why you recommend this choice, on the recommended choice only. The reviewer sees it as
    /// your recommendation; on a question whose Door is not two-way, only after a first pick of
    /// their own.
    pub recommendation: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Question {
    /// Stable question ID that names its subject, such as `cache-eviction`, never a number:
    /// the reviewer's screens number the questions. Use a higher version only to clarify the
    /// same question.
    pub id: String,
    #[schemars(range(min = 1))]
    pub version: u32,
    /// ID of an active topic in this round.
    pub topic: String,
    pub text: String,
    /// Context section body: short summary paragraph, then optional Markdown explanation.
    /// Explain relevant behavior and unfamiliar terms without assuming implementation knowledge.
    /// For a behavior change, say what changes, then why, then how.
    pub rationale: Option<String>,
    /// Optional simplified/proposed sketch appended to Context; use a fenced Markdown block.
    pub visual: Option<String>,
    /// Distinct choices; the reviewer adds None of the above automatically.
    #[schemars(length(min = 2, max = 5))]
    pub alternatives: Vec<Alternative>,
    /// The lines the question is about, most decisive first, each with notes
    /// explaining what it shows and why it matters to the decision.
    pub evidence: Vec<EvidenceRef>,
    pub assessments: Option<Assessments>,
}

impl Topic {
    pub(crate) fn same_but_status(&self, other: &Self) -> bool {
        *self
            == Self {
                status: self.status,
                ..other.clone()
            }
    }
}

impl Question {
    pub fn is_version(&self, id: &str, version: u32) -> bool {
        self.id == id && self.version == version
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ReviewerAnswer {
    pub id: String,
    pub checkpoint: ReviewCheckpoint,
    pub question: Option<Question>,
    pub in_reply_to: String,
    pub option: Option<Alternative>,
    pub text: String,
    pub author: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_pick: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub drawing_board: bool,
}

impl ReviewerAnswer {
    pub fn sends_to_drawing_board(&self) -> bool {
        self.drawing_board && self.question.is_some()
    }

    pub(crate) fn answers(&self, question: &Question) -> bool {
        self.question.as_ref() == Some(question)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AnswerInput {
    pub option: Option<String>,
    pub text: String,
    pub in_reply_to: Option<String>,
    pub first_pick: Option<String>,
    pub drawing_board: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Interpretation {
    /// Exact Answer ID from the latest wakeup.
    pub answer: String,
    /// Preserve an unqualified choice's outcome; required changes mean `needs_follow_up`.
    pub status: TopicStatus,
    pub recap: String,
    pub follow_ups: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InterviewUpdate {
    /// Copy Explore round from the latest wakeup.
    pub instance: String,
    /// Copy Explore request from the latest wakeup.
    pub request: String,
    /// Copy Review unit and Checkpoint from the latest wakeup.
    pub checkpoint: ReviewCheckpoint,
    /// Interpret only the latest human decision. Null for kickoff or factual context.
    pub interpretation: Option<Interpretation>,
    /// After a human answer: the changed lines it settled, to mark reviewed. Any
    /// changed lines, cited or not; null lines mark the whole file.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reviewed: Vec<CodeLocation>,
    /// After a human answer: reviewed lines it makes matter again, to reopen. Any
    /// reviewed lines, whoever marked them; null lines reopen the whole file.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reopened: Vec<CodeLocation>,
    /// On any turn: changed lines you read that hold no decision for the
    /// reviewer, to mark reviewed, each with its reason; null lines mark the
    /// whole file.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub not_relevant: Vec<crate::NotRelevantMark>,
    /// In a round with a challenger: what became of each question it
    /// proposed on this turn.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub challenger_proposals: Vec<crate::ChallengerProposal>,
    pub reply: Option<Reply>,
    /// On the first turn, before its question: the design of the change. Null on later turns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub design: Option<crate::Design>,
    /// The terms of this change that a reviewer new to the code may not know, each with its
    /// definition in one plain sentence: the reviewer's screens mark every use of a term and
    /// show its definition on demand, so the texts use the term without defining it. Send only
    /// new terms and changed definitions; the round keeps those sent before.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub glossary: BTreeMap<String, String>,
    #[serde(default)]
    pub agenda: Vec<AgendaChange>,
    pub topics: Vec<Topic>,
    /// Required for `submit_question`. Use `submit_conclusion` to finish the interview.
    #[schemars(required)]
    pub next: Option<Question>,
    #[schemars(skip)]
    pub conclusion: Option<crate::Conclusion>,
    pub limitations: Vec<String>,
    pub findings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct TurnRequest {
    pub instance: String,
    pub request: String,
    pub checkpoint: ReviewCheckpoint,
    pub answer: Option<ReviewerAnswer>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cancelled: Vec<String>,
    pub response_error: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub challenger: bool,
    #[serde(default, skip_serializing_if = "ChangeAuthor::is_reviewer")]
    pub author: ChangeAuthor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewer: Option<String>,
    #[serde(
        default = "before_writing_styles",
        skip_serializing_if = "WritingStyle::is_plain"
    )]
    pub writing: WritingStyle,
}

fn before_writing_styles() -> WritingStyle {
    WritingStyle::Plain
}

impl TurnRequest {
    pub fn is_kickoff(&self) -> bool {
        self.answer.is_none()
    }

    pub fn at_drawing_board(&self) -> bool {
        self.answer
            .as_ref()
            .is_some_and(|answer| answer.drawing_board)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct Exploration {
    pub instance: String,
    pub comparison: Arc<Comparison>,
    pub topics: BTreeMap<String, Topic>,
    pub questions: Vec<Question>,
    pub answers: Vec<ReviewerAnswer>,
    pub interpretations: Vec<Interpretation>,
    pub limitations: Vec<String>,
    pub findings: Vec<String>,
    pub conclusion: Option<crate::Conclusion>,
    pub conversation: Vec<ConversationTurn>,
    #[serde(default)]
    pub agent_elapsed_ms: BTreeMap<String, u64>,
    pub(crate) outstanding: Option<TurnRequest>,
    pub(crate) retry: Option<TurnRequest>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) cancelled: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub challenger: bool,
    #[serde(default, skip_serializing_if = "ChangeAuthor::is_reviewer")]
    pub author: ChangeAuthor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewer: Option<String>,
    #[serde(
        default = "before_writing_styles",
        skip_serializing_if = "WritingStyle::is_plain"
    )]
    pub writing: WritingStyle,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub glossary: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagram_errors: Vec<crate::DiagramError>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) quiz_answers: BTreeMap<String, crate::QuizAnswers>,
}

impl Exploration {
    pub fn new(comparison: Arc<Comparison>) -> Self {
        Self {
            instance: uuid::Uuid::new_v4().to_string(),
            comparison,
            topics: BTreeMap::new(),
            questions: Vec::new(),
            answers: Vec::new(),
            interpretations: Vec::new(),
            limitations: Vec::new(),
            findings: Vec::new(),
            conclusion: None,
            conversation: Vec::new(),
            agent_elapsed_ms: BTreeMap::new(),
            outstanding: None,
            retry: None,
            cancelled: Vec::new(),
            challenger: false,
            author: ChangeAuthor::Reviewer,
            reviewer: None,
            writing: before_writing_styles(),
            glossary: BTreeMap::new(),
            diagram_errors: Vec::new(),
            quiz_answers: BTreeMap::new(),
        }
    }

    pub(crate) fn topic_status(&self, id: &str) -> Option<TopicStatus> {
        self.topics.get(id).map(|topic| topic.status)
    }

    fn pending(&self) -> bool {
        self.outstanding.is_some()
    }

    pub fn cancelled_since_last_turn(&self) -> bool {
        !self.cancelled.is_empty()
    }

    pub fn took_up(&self, answer: &str) -> bool {
        self.turn_after(answer).is_some()
    }

    pub(crate) fn turn_after(&self, answer: &str) -> Option<&ConversationTurn> {
        self.conversation
            .iter()
            .find(|turn| turn.answer.as_deref() == Some(answer))
    }

    pub fn interpretation(&self, answer: &str) -> Option<&Interpretation> {
        self.interpretations
            .iter()
            .rev()
            .find(|interpretation| interpretation.answer == answer)
    }

    pub fn request(
        &mut self,
        input: Option<AnswerInput>,
        question: Option<&Question>,
    ) -> eyre::Result<TurnRequest> {
        eyre::ensure!(!self.pending(), "An interview request is already pending");
        let answer = input
            .map(|input| self.record(input, question))
            .transpose()?;
        let mut cancelled = self
            .retry
            .take()
            .map(|retry| retry.cancelled)
            .unwrap_or_default();
        cancelled.append(&mut self.cancelled);
        let request = TurnRequest {
            instance: self.instance.clone(),
            request: uuid::Uuid::new_v4().to_string(),
            checkpoint: self.comparison.checkpoint.clone(),
            answer,
            cancelled,
            response_error: None,
            challenger: self.challenger,
            author: self.author,
            reviewer: self.reviewer.clone(),
            writing: self.writing,
        };
        self.outstanding = Some(request.clone());
        self.retry = Some(request.clone());
        Ok(request)
    }

    fn record(
        &mut self,
        input: AnswerInput,
        question: Option<&Question>,
    ) -> eyre::Result<ReviewerAnswer> {
        let context = self
            .conversation
            .iter()
            .rev()
            .find(|turn| match question {
                Some(question) => turn.update.next.as_ref() == Some(question),
                None => {
                    turn.update.conclusion.is_some()
                        && input
                            .in_reply_to
                            .as_ref()
                            .is_none_or(|id| id == &turn.update.request)
                }
            })
            .ok_or_else(|| eyre::eyre!("Unknown question or conversation context"))?;
        let in_reply_to = context.update.request.clone();
        let replies_at_drawing_board = question.is_none()
            && context.answer.as_ref().is_some_and(|answer| {
                self.answers
                    .iter()
                    .any(|earlier| &earlier.id == answer && earlier.drawing_board)
            });
        eyre::ensure!(
            question.is_some() || input.option.is_none(),
            "A conversation reply has no policy choices"
        );
        if input.drawing_board {
            eyre::ensure!(
                question.is_some() && input.option.is_none(),
                "Back to the drawing board takes the place of an answer to the question, with \
                 no choice"
            );
        } else {
            eyre::ensure!(
                input.option.is_some() || !input.text.trim().is_empty(),
                "Write an answer or choose an option"
            );
        }
        let option = input
            .option
            .as_ref()
            .map(|id| {
                question
                    .ok_or_else(|| eyre::eyre!("No question is selected"))?
                    .choice(id)
                    .cloned()
                    .ok_or_else(|| eyre::eyre!("Unknown option"))
            })
            .transpose()?;
        if input.option.is_some() {
            eyre::ensure!(
                question.is_some_and(|question| self.can_choose(question)),
                "This choice is no longer pending; reply in free text"
            );
        }
        if let Some(first_pick) = &input.first_pick {
            eyre::ensure!(
                question.is_some_and(|question| question.choice(first_pick).is_some()),
                "The first pick is not a choice of this question"
            );
        }
        let answer = ReviewerAnswer {
            id: uuid::Uuid::new_v4().to_string(),
            checkpoint: self.comparison.checkpoint.clone(),
            question: question.cloned(),
            in_reply_to,
            option,
            text: input.text,
            author: "reviewer".into(),
            first_pick: input.first_pick,
            drawing_board: input.drawing_board || replies_at_drawing_board,
        };
        self.answers.push(answer.clone());
        Ok(answer)
    }

    pub fn cancel(&mut self) {
        self.outstanding = None;
    }

    pub fn failed(&mut self, request: &str, error: &str) -> bool {
        if self
            .outstanding
            .as_ref()
            .is_none_or(|turn| turn.request != request)
        {
            return false;
        }
        self.outstanding = None;
        if let Some(retry) = &mut self.retry {
            retry.response_error = Some(error.to_owned());
        }
        true
    }

    pub fn retry(&mut self) -> eyre::Result<TurnRequest> {
        eyre::ensure!(!self.pending(), "An interview request is already pending");
        let request = self
            .retry
            .clone()
            .ok_or_else(|| eyre::eyre!("No request to retry"))?;
        self.retry = Some(request.clone());
        self.outstanding = Some(request.clone());
        Ok(request)
    }

    pub fn apply(&mut self, update: InterviewUpdate) -> eyre::Result<bool> {
        let Some(request) = &self.outstanding else {
            return Ok(false);
        };
        if request.instance != update.instance || request.request != update.request {
            return Ok(false);
        }
        self.validate(&update, &self.comparison)?;
        let answer = request.answer.clone();
        self.conversation.push(ConversationTurn {
            answer: answer.as_ref().map(|answer| answer.id.clone()),
            update: update.clone(),
            revision: None,
        });
        self.absorb(update, answer.as_ref());
        self.outstanding = None;
        self.retry = None;
        Ok(true)
    }

    pub(crate) fn absorb(&mut self, update: InterviewUpdate, answer: Option<&ReviewerAnswer>) {
        self.glossary.extend(update.glossary);
        for mut topic in update.topics {
            topic.status = self.topic_status(&topic.id).unwrap_or_default();
            self.topics.insert(topic.id.clone(), topic);
        }
        if let Some((interpretation, answer)) = update.interpretation.zip(answer) {
            if let Some(question) = &answer.question
                && let Some(topic) = self.topics.get_mut(&question.topic)
            {
                topic.status = interpretation.status;
            }
            self.interpretations.push(interpretation);
        }
        if let Some(question) = update.next {
            self.questions.push(question);
        }
        self.record_report(
            update.limitations,
            update.findings,
            update.conclusion.is_some(),
        );
        self.conclusion = update.conclusion;
    }

    fn record_report(&mut self, limitations: Vec<String>, findings: Vec<String>, concluded: bool) {
        if !concluded || !limitations.is_empty() {
            self.limitations = limitations;
        }
        for finding in findings {
            if !self.findings.contains(&finding) {
                self.findings.push(finding);
            }
        }
    }

    pub fn submit(&mut self, update: InterviewUpdate) -> eyre::Result<bool> {
        eyre::ensure!(
            update.instance == self.instance,
            "This Explore round is no longer active"
        );
        if let Some(previous) = self
            .conversation
            .iter()
            .find(|turn| turn.update.request == update.request)
        {
            eyre::ensure!(
                previous.update == update,
                "An accepted turn cannot be replaced; retry the identical payload"
            );
            return Ok(false);
        }
        eyre::ensure!(
            self.outstanding
                .as_ref()
                .is_some_and(|request| request.request == update.request),
            "This Explore request is cancelled or obsolete"
        );
        self.apply(update)
    }

    pub fn unmapped(&self) -> Vec<&crate::ManifestEntry> {
        self.comparison
            .manifest
            .iter()
            .filter(|entry| {
                !self.topics.values().any(|topic| {
                    topic
                        .entries
                        .iter()
                        .any(|location| self.comparison.maps(entry, location))
                })
            })
            .collect()
    }
}
