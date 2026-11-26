use review_round_conversation::{ConversationMessage, Delivery, RoundConversation};
use review_threads::{AskedUnder, Author};
use serde::Serialize;
use ts_rs::TS;

use super::markdown;
use crate::conversation::ThreadsSnapshot;
use crate::round::RoundSnapshot;
use crate::status::StatusCard;

#[derive(Debug, Serialize, TS)]
pub(crate) struct ConversationView {
    round: String,
    messages: Vec<ChatMessageView>,
    unread: usize,
    read_through: u64,
    card: Option<StatusCard>,
    writable: bool,
}

#[derive(Debug, Serialize, TS)]
struct ChatMessageView {
    id: String,
    author: Author,
    html: String,
    posted_at_ms: Option<u64>,
    place: Option<String>,
    quote: Option<String>,
    delivery: Option<Delivery>,
    unread: bool,
}

impl ConversationView {
    pub(crate) fn of(round: &RoundSnapshot, threads: &ThreadsSnapshot) -> Option<Self> {
        let (id, unit) = round.conversation_round()?;
        let conversation = threads.conversation(unit, id);
        Some(Self::new(round, conversation))
    }

    fn new(round: &RoundSnapshot, conversation: RoundConversation) -> Self {
        let undelivered =
            conversation
                .messages
                .iter()
                .find_map(|message| match &message.delivery {
                    Some(Delivery::NotDelivered { error }) => Some(error.clone()),
                    _ => None,
                });
        let writable = !round.earlier;
        Self {
            card: undelivered
                .filter(|_| writable && conversation.can_retry)
                .map(|error| StatusCard::undelivered_messages(&error, &conversation.round)),
            messages: conversation
                .messages
                .into_iter()
                .map(|message| ChatMessageView::new(round, message))
                .collect(),
            unread: conversation.unread,
            read_through: conversation.read_through,
            round: conversation.round,
            writable,
        }
    }
}

impl ChatMessageView {
    fn new(round: &RoundSnapshot, message: ConversationMessage) -> Self {
        Self {
            id: message.id.as_str().to_owned(),
            author: message.author,
            html: markdown(&message.text, 3),
            posted_at_ms: message.posted_at_ms,
            place: message
                .asked_under
                .as_ref()
                .and_then(|asked| place(round, asked)),
            quote: message.quote,
            delivery: message.delivery,
            unread: message.unread,
        }
    }
}

fn place(round: &RoundSnapshot, asked: &AskedUnder) -> Option<String> {
    match asked {
        AskedUnder::Question {
            question, version, ..
        } => round
            .number_of_question(question, *version)
            .map(|number| review_types::QuestionNumber::from(number).to_string()),
        AskedUnder::Design => Some("Design".to_owned()),
        AskedUnder::Conclusion { .. } => Some("Conclusion".to_owned()),
    }
}
