use std::ops::Not;

use agent_fork::ProcessStamp;
use review_explore::InterviewUpdate;
use review_turn_path::{PlainReason, TurnPath};
use serde::{Deserialize, Serialize};

use crate::answer::AnswerRecord;
use crate::host::HostFields;

pub const FAILURES_TO_HALT: u32 = 3;

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub struct RoundForks {
    pub forks: Vec<ForkRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub answers: Vec<AnswerRecord>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub failed_in_a_row: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub halted_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub halted_by: Option<Failure>,
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
fn is_zero(count: &u32) -> bool {
    *count == 0
}

impl RoundForks {
    pub fn fork(&self, session: &str) -> Option<&ForkRecord> {
        self.forks.iter().find(|fork| fork.session == session)
    }

    pub fn fork_mut(&mut self, session: &str) -> Option<&mut ForkRecord> {
        self.forks.iter_mut().find(|fork| fork.session == session)
    }

    pub fn answer_mut(&mut self, request: &str) -> Option<&mut AnswerRecord> {
        self.answers
            .iter_mut()
            .find(|answer| answer.request == request)
    }

    pub fn failed(&mut self, failure: Failure, at_ms: u64) -> bool {
        self.failed_in_a_row += 1;
        if self.halted_at_ms.is_some() || self.failed_in_a_row < FAILURES_TO_HALT {
            return false;
        }
        self.halted_at_ms = Some(at_ms);
        self.halted_by = Some(failure);
        true
    }

    pub fn prepared_turn_used(&mut self) {
        self.failed_in_a_row = 0;
    }

    pub fn halt_notice(&self) -> Option<String> {
        self.halted_at_ms?;
        let stopped =
            format!("Run-ahead stopped for this round after {FAILURES_TO_HALT} failures in a row");
        Some(match &self.halted_by {
            Some(failure) => format!("{stopped}; the last: {failure}"),
            None => stopped,
        })
    }

    pub fn continued_as(&self, session: &str) -> Option<&str> {
        self.answers.iter().find_map(|answer| match &answer.path {
            TurnPath::Prepared { session: prepared } if prepared == session => {
                Some(answer.request.as_str())
            }
            _ => None,
        })
    }

    pub fn turn_paths(&self) -> impl Iterator<Item = (&str, &TurnPath)> {
        self.answers.iter().filter_map(|answer| {
            let shown =
                matches!(answer.path, TurnPath::Plain { .. }) || self.used_a_prepared_turn(answer);
            shown.then_some((answer.request.as_str(), &answer.path))
        })
    }

    pub fn used_a_prepared_turn(&self, answer: &AnswerRecord) -> bool {
        match &answer.path {
            TurnPath::Prepared { session } => {
                self.fork(session).is_some_and(ForkRecord::is_continued)
            }
            TurnPath::Plain { .. } => false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ForkRecord {
    pub question: String,
    pub version: u32,
    pub choice: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
    pub session: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    pub reviewer: ProcessStamp,
    pub taken_at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn: Option<Box<InterviewUpdate>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submitted_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discarded: Option<Discard>,
    #[serde(default, skip_serializing_if = "Not::not")]
    pub cleaned: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continued: Option<Continuation>,
    #[serde(flatten)]
    pub host: HostFields,
}

impl ForkRecord {
    pub fn trace(&self) -> crate::ForkTrace<'_> {
        crate::ForkTrace {
            session: &self.session,
            host: &self.host,
        }
    }

    pub fn is_continued(&self) -> bool {
        matches!(self.continued, Some(Continuation::Switched { .. }))
    }

    pub fn is_agent_session(&self) -> bool {
        match self.continued {
            Some(Continuation::Switching { .. } | Continuation::Switched { .. }) => true,
            Some(Continuation::Failed { typed, .. }) => typed,
            Some(Continuation::Undone { .. } | Continuation::Restarted { .. }) | None => false,
        }
    }

    pub fn is_unsettled(&self) -> bool {
        matches!(
            self.continued,
            Some(Continuation::Switching { .. } | Continuation::Failed { typed: true, .. })
        )
    }

    pub fn ending_is_a_failure(&self) -> bool {
        self.turn.is_none() && self.discarded.is_none()
    }

    pub fn left_by_a_stopped_reviewer(&self, reviewer: ProcessStamp) -> bool {
        !self.cleaned
            && !self.is_agent_session()
            && self.reviewer != reviewer
            && !self.reviewer.is_running()
    }

    pub fn runs_for_another_reviewer(&self, reviewer: ProcessStamp) -> bool {
        self.discarded.is_none()
            && !self.cleaned
            && self.reviewer != reviewer
            && self.reviewer.is_running()
    }

    pub fn may_be_settled_by(&self, reviewer: ProcessStamp) -> bool {
        self.reviewer == reviewer || !self.reviewer.is_running()
    }

    pub fn discard(&mut self, reason: DiscardReason, at_ms: u64) {
        if self.discarded.is_some() || self.is_agent_session() {
            return;
        }
        self.discarded = Some(Discard {
            reason,
            prepared: self.turn.take().is_some(),
            at_ms,
        });
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Continuation {
    Switching {
        at_ms: u64,
    },
    Switched {
        at_ms: u64,
    },
    Failed {
        at_ms: u64,
        error: String,
        typed: bool,
    },
    Undone {
        at_ms: u64,
        reason: String,
    },
    Restarted {
        at_ms: u64,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(tag = "failure", rename_all = "snake_case")]
pub enum Failure {
    Fork { exit: String },
    Unchecked { error: String },
    SwitchFailed { error: String },
    NoForks { why: String },
}

impl Failure {
    pub fn of_plain(reason: &PlainReason) -> Option<Self> {
        match reason {
            PlainReason::Unchecked { error } => Some(Self::Unchecked {
                error: error.clone(),
            }),
            PlainReason::SwitchFailed { error } => Some(Self::SwitchFailed {
                error: error.clone(),
            }),
            _ => None,
        }
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fork { exit } => write!(f, "a fork ended without a turn: {exit}"),
            Self::Unchecked { error } => {
                write!(f, "a check of your answer could not be made: {error}")
            }
            Self::SwitchFailed { error } => {
                write!(
                    f,
                    "the agent could not switch to the prepared turn: {error}"
                )
            }
            Self::NoForks { why } => write!(f, "no fork could be taken: {why}"),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct Discard {
    pub reason: DiscardReason,
    pub prepared: bool,
    pub at_ms: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DiscardReason {
    Answered,
    StoppedWaiting,
    Withdrawn,
    AnswerCancelled,
    QuestionGone,
    SessionMoved,
    ChatMessage,
    TurnedOff,
    Reset,
    NewRound,
    ReviewChanged,
    ReviewerClosed,
    ReviewerStopped,
    TooManyFailures,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub struct TokenUsage {
    pub input: u64,
    pub cache_creation: u64,
    pub cache_read: u64,
    pub output: u64,
}

impl TokenUsage {
    pub fn total(self) -> u64 {
        self.input + self.cache_creation + self.cache_read + self.output
    }
}

impl std::ops::AddAssign for TokenUsage {
    fn add_assign(&mut self, other: Self) {
        self.input += other.input;
        self.cache_creation += other.cache_creation;
        self.cache_read += other.cache_read;
        self.output += other.output;
    }
}

#[cfg(test)]
#[path = "record.tests.rs"]
mod tests;
