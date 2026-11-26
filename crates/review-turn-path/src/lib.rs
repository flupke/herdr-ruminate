use std::borrow::Cow;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(tag = "path", rename_all = "snake_case")]
pub enum TurnPath {
    Prepared { session: String },
    Plain { reason: PlainReason },
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum PlainReason {
    NoHooks,
    Comment,
    NoneOfTheAbove,
    DrawingBoard,
    NoForks {
        why: Option<String>,
    },
    NotForked,
    StillWorking,
    NoTurn,
    StoppedWaiting,
    ChatMessage,
    AgentBusy,
    SessionMoved,
    UnreviewedChanged,
    PromptChanged,
    Unchecked {
        error: String,
    },
    SwitchFailed {
        error: String,
    },
    Withdrawn,
    #[serde(other)]
    Obsolete,
}

impl TurnPath {
    pub fn line(&self) -> Option<Cow<'static, str>> {
        match self {
            Self::Prepared { .. } => Some("Prepared while you were thinking".into()),
            Self::Plain {
                reason: PlainReason::NoForks { why: Some(why) },
            } => {
                Some(format!("Not prepared: nothing was prepared for this question ({why})").into())
            }
            Self::Plain { reason } => reason.line().map(Cow::Borrowed),
        }
    }
}

impl PlainReason {
    fn line(&self) -> Option<&'static str> {
        Some(match self {
            Self::NoHooks => {
                "Not prepared: restart the agent to load the reviewer's Claude Code plugin"
            }
            Self::Comment => "Not prepared: you added a comment",
            Self::NoneOfTheAbove => "Not prepared: you answered None of the above",
            Self::DrawingBoard => "Not prepared: you sent the agent back to the drawing board",
            Self::ChatMessage => "Not prepared: you wrote in the chat",
            Self::AgentBusy => "Not prepared: the agent was working",
            Self::SessionMoved => "Not prepared: the agent's conversation moved on",
            Self::UnreviewedChanged => "Not prepared: the lines left to review changed",
            Self::PromptChanged => "Not prepared: the round changed after it was prepared",
            other => return other.not_ready_line(),
        })
    }

    fn not_ready_line(&self) -> Option<&'static str> {
        match self {
            Self::NoForks { .. } => Some("Not prepared: nothing was prepared for this question"),
            Self::NotForked => Some("Not prepared: only the recommended choice is prepared"),
            Self::StillWorking => Some("Not prepared: the turn for this choice was not ready yet"),
            Self::NoTurn => Some("Not prepared: the turn for this choice did not finish"),
            Self::StoppedWaiting => Some("Not prepared: you stopped waiting for it"),
            Self::SwitchFailed { .. } => {
                Some("Not prepared: the agent could not switch to the prepared turn")
            }
            Self::Withdrawn => Some("Not prepared: you stopped waiting while it was being used"),
            Self::Unchecked { .. }
            | Self::Obsolete
            | Self::NoHooks
            | Self::Comment
            | Self::NoneOfTheAbove
            | Self::DrawingBoard
            | Self::ChatMessage
            | Self::AgentBusy
            | Self::SessionMoved
            | Self::UnreviewedChanged
            | Self::PromptChanged => None,
        }
    }
}

#[cfg(test)]
#[path = "lib.tests.rs"]
mod tests;
