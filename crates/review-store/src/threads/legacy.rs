use std::collections::BTreeMap;

use review_threads::{Author, MessageId, ThreadId};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct RecipientCursors {
    threads: Vec<CursorThread>,
    #[serde(default)]
    readers: BTreeMap<String, BTreeMap<ThreadId, u64>>,
    #[serde(default)]
    answered: BTreeMap<ThreadId, u64>,
}

#[derive(Deserialize)]
struct CursorThread {
    id: ThreadId,
    messages: Vec<CursorMessage>,
}

#[derive(Deserialize)]
struct CursorMessage {
    id: MessageId,
    author: Author,
    #[serde(default)]
    in_reply_to: Option<MessageId>,
    sequence: u64,
}

impl CursorThread {
    fn last_reply_sequence(&self) -> u64 {
        self.messages
            .iter()
            .rev()
            .find(|message| message.author == Author::Agent)
            .map_or(0, |message| message.sequence)
    }

    fn answered_through(&self) -> u64 {
        self.messages
            .iter()
            .filter(|message| message.author == Author::Agent)
            .filter_map(|message| message.in_reply_to.as_ref())
            .filter_map(|id| {
                self.messages
                    .iter()
                    .find(|message| message.id == *id && message.author == Author::Reviewer)
            })
            .map(|message| message.sequence)
            .max()
            .unwrap_or_default()
    }
}

impl RecipientCursors {
    fn recover_retrieved_comments(&mut self) {
        for positions in self.readers.values_mut() {
            for (id, through) in positions {
                let answered = self
                    .threads
                    .iter()
                    .find(|thread| thread.id == *id)
                    .map_or(0, CursorThread::last_reply_sequence);
                if *through >= answered {
                    *through = 0;
                }
            }
        }
    }

    fn into_answered(mut self) -> BTreeMap<ThreadId, u64> {
        for positions in std::mem::take(&mut self.readers).into_values() {
            for (thread, through) in positions {
                let position = self.answered.entry(thread).or_default();
                *position = (*position).max(through);
            }
        }
        for thread in &self.threads {
            let position = self.answered.entry(thread.id.clone()).or_default();
            *position = (*position).max(thread.answered_through());
        }
        self.answered
    }
}

pub(super) fn migrate(conversations: &mut Value, version: u8) -> serde_json::Result<()> {
    let mut cursors = RecipientCursors::deserialize(&*conversations)?;
    if version == 2 {
        cursors.recover_retrieved_comments();
    }
    let answered = serde_json::to_value(cursors.into_answered())?;
    if let Value::Object(fields) = conversations {
        fields.remove("readers");
        fields.insert("answered".into(), answered);
    }
    Ok(())
}
