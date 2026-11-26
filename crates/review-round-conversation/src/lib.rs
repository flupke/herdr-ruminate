use review_threads::{
    AskedUnder, Author, Message, MessageId, ReviewThread, ReviewThreads, ThreadId, WakeupNotice,
};
use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RoundConversation {
    pub round: String,
    pub thread: Option<ThreadId>,
    pub messages: Vec<ConversationMessage>,
    pub unread: usize,
    pub can_retry: bool,
    pub read_through: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ConversationMessage {
    pub id: MessageId,
    pub author: Author,
    pub text: String,
    pub posted_at_ms: Option<u64>,
    pub asked_under: Option<AskedUnder>,
    pub quote: Option<String>,
    pub delivery: Option<Delivery>,
    pub unread: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, ts_rs::TS)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Delivery {
    Waiting { held: Option<String> },
    NotDelivered { error: String },
    Answered,
}

impl RoundConversation {
    pub fn new(threads: &ReviewThreads, round: &str, notice: Option<&WakeupNotice>) -> Self {
        let thread = threads.round_conversation(round);
        Self {
            round: round.to_owned(),
            thread: thread.map(|thread| thread.id.clone()),
            messages: thread
                .map(|thread| {
                    thread
                        .messages
                        .iter()
                        .map(|message| ConversationMessage::new(thread, message, notice))
                        .collect()
                })
                .unwrap_or_default(),
            unread: thread.map_or(0, |thread| {
                thread
                    .messages
                    .iter()
                    .filter(|message| thread.has_unread_reply(&message.id))
                    .count()
            }),
            can_retry: thread.is_some_and(|thread| threads.retry(&thread.id).is_ok()),
            read_through: threads.sequence(),
        }
    }
}

impl ConversationMessage {
    fn new(thread: &ReviewThread, message: &Message, notice: Option<&WakeupNotice>) -> Self {
        let delivery = (message.author == Author::Reviewer).then(|| {
            if thread.is_answered(message) {
                Delivery::Answered
            } else {
                match notice {
                    Some(WakeupNotice::Failed(failure))
                        if message.sequence() <= failure.through =>
                    {
                        Delivery::NotDelivered {
                            error: failure.error.clone(),
                        }
                    }
                    Some(WakeupNotice::Waiting(wait)) if message.sequence() <= wait.through => {
                        Delivery::Waiting {
                            held: Some(wait.reason.clone()),
                        }
                    }
                    _ => Delivery::Waiting { held: None },
                }
            }
        });
        Self {
            id: message.id.clone(),
            author: message.author,
            text: message.text.clone(),
            posted_at_ms: message.posted_at_ms,
            asked_under: message.asked_under.clone(),
            quote: message.quote.clone(),
            delivery,
            unread: thread.has_unread_reply(&message.id),
        }
    }
}

#[cfg(test)]
#[path = "lib.tests.rs"]
mod tests;
