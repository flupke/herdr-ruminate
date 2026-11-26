use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Message, MessageId, Post, ReviewThread, ReviewThreads, ThreadId, ThreadSubject};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum AskedUnder {
    Question {
        question: String,
        version: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional, as = "Option<usize>")]
        number: Option<review_types::QuestionNumber>,
    },
    Design,
    Conclusion {
        conclusion: String,
    },
}

impl ThreadId {
    fn of_round(round: &str) -> Self {
        Self(format!("explore-round-{round}"))
    }
}

impl Post {
    pub fn to_round(
        round: &str,
        text: String,
        asked_under: Option<AskedUnder>,
        quote: Option<String>,
    ) -> Self {
        let mut message = Message::reviewer(text);
        message.asked_under = asked_under;
        message.quote = quote;
        Self {
            thread_id: ThreadId::of_round(round),
            message,
            subject: Some(ThreadSubject::Round {
                round: round.to_owned(),
            }),
        }
    }

    #[must_use]
    pub fn with_id(mut self, id: MessageId) -> Self {
        self.message.id = id;
        self
    }
}

impl ReviewThread {
    pub fn round(&self) -> Option<&str> {
        match &self.subject {
            ThreadSubject::Round { round } => Some(round),
            ThreadSubject::Code(_) => None,
        }
    }
}

impl ReviewThreads {
    pub fn round_conversation(&self, round: &str) -> Option<&ReviewThread> {
        self.thread(&ThreadId::of_round(round))
    }
}

#[cfg(test)]
#[path = "round.tests.rs"]
mod tests;
