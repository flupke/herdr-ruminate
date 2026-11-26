mod attention;
mod command;
mod draft;
mod merge;
mod message;
mod paths;
mod post;
mod round;
mod source;
mod wakeup;

use std::collections::{BTreeMap, BTreeSet};

use review_types::ReviewUnit;
use serde::{Deserialize, Serialize};

pub use attention::{Resolution, ThreadCounts};
pub use command::ThreadCommand;
pub use draft::{Draft, DraftTarget, SavedDrafts};
pub use message::{Author, Message, MessageId};
pub use paths::ThreadPaths;
pub use post::Post;
pub use round::AskedUnder;
pub use source::{ThreadSource, ThreadSubject};
pub use wakeup::{WakeupFailure, WakeupNotice, WakeupWait};

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ThreadId(String);

impl ThreadId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReviewThread {
    pub id: ThreadId,
    #[serde(flatten)]
    pub subject: ThreadSubject,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub resolution: Resolution,
    #[serde(default)]
    seen_reply_through: u64,
    #[serde(default)]
    seen_replies: BTreeSet<MessageId>,
}

impl ReviewThread {
    pub fn last_comment(&self) -> Option<&Message> {
        self.messages
            .iter()
            .rev()
            .find(|message| message.author == Author::Reviewer)
    }

    fn receive(&mut self, message: Message) {
        if message.author == Author::Reviewer && self.round().is_some() {
            self.resolution = Resolution::Open;
        }
        self.messages.push(message);
    }

    fn has_comments_after(&self, sequence: u64) -> bool {
        self.messages
            .iter()
            .any(|message| message.author == Author::Reviewer && message.sequence > sequence)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Recipient<'a> {
    pub tab: &'a str,
    pub retried: &'a BTreeMap<ThreadId, u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReviewThreads {
    pub review_unit: ReviewUnit,
    threads: Vec<ReviewThread>,
    sequence: u64,
    #[serde(default)]
    answered: BTreeMap<ThreadId, u64>,
}

impl ReviewThreads {
    pub fn new(review_unit: ReviewUnit) -> Self {
        Self {
            review_unit,
            threads: Vec::new(),
            sequence: 0,
            answered: BTreeMap::new(),
        }
    }

    pub fn threads(&self) -> &[ReviewThread] {
        &self.threads
    }

    pub fn thread(&self, id: &ThreadId) -> Option<&ReviewThread> {
        self.threads.iter().find(|thread| &thread.id == id)
    }

    pub fn thread_for_message(&self, id: &MessageId) -> Option<&ReviewThread> {
        self.threads
            .iter()
            .find(|thread| thread.messages.iter().any(|message| &message.id == id))
    }

    pub fn message(&self, id: &MessageId) -> Option<&Message> {
        self.thread_for_message(id)?
            .messages
            .iter()
            .find(|message| &message.id == id)
    }

    pub fn reply_count(&self) -> usize {
        self.threads
            .iter()
            .flat_map(|thread| &thread.messages)
            .filter(|message| message.author == Author::Agent)
            .count()
    }

    pub fn stamp_postings(&mut self, time: impl Fn(u64) -> u64) {
        for message in self
            .threads
            .iter_mut()
            .flat_map(|thread| &mut thread.messages)
        {
            message.posted_at_ms = Some(time(message.sequence()));
        }
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    fn pending(&self, thread: &ReviewThread) -> bool {
        thread.resolution == Resolution::Open
            && thread.is_waiting()
            && thread.has_comments_after(self.answered.get(&thread.id).copied().unwrap_or_default())
    }

    pub fn new_messages(&self) -> Vec<ReviewThread> {
        self.threads
            .iter()
            .filter(|thread| self.pending(thread))
            .cloned()
            .collect()
    }

    pub fn has_new_messages(&self) -> bool {
        self.threads.iter().any(|thread| self.pending(thread))
    }

    fn pending_for(&self, thread: &ReviewThread, recipient: &Recipient<'_>) -> bool {
        let answered = self.answered.get(&thread.id).copied().unwrap_or_default();
        let retried = recipient
            .retried
            .get(&thread.id)
            .copied()
            .unwrap_or_default();
        self.pending(thread)
            && thread.messages.iter().any(|message| {
                message.author == Author::Reviewer
                    && message.sequence > answered
                    && !thread.is_answered(message)
                    && (message.sequence <= retried
                        || message.posted_from_tab.as_deref() == Some(recipient.tab))
            })
    }

    pub fn new_messages_for(&self, recipient: &Recipient<'_>) -> Vec<ReviewThread> {
        self.threads
            .iter()
            .filter(|thread| self.pending_for(thread, recipient))
            .cloned()
            .collect()
    }

    pub fn has_new_messages_for(&self, recipient: &Recipient<'_>) -> bool {
        self.threads
            .iter()
            .any(|thread| self.pending_for(thread, recipient))
    }

    pub fn pending_comment_sequence_for(&self, recipient: &Recipient<'_>) -> Option<u64> {
        self.threads
            .iter()
            .filter(|thread| self.pending_for(thread, recipient))
            .filter_map(ReviewThread::last_comment)
            .map(|message| message.sequence)
            .max()
    }

    pub fn pending_comment_sequence(&self) -> Option<u64> {
        self.threads
            .iter()
            .filter(|thread| self.pending(thread))
            .filter_map(ReviewThread::last_comment)
            .map(|message| message.sequence)
            .max()
    }

    pub fn answer(&mut self, post: Post) -> Result<MessageId, String> {
        if post.message.author != Author::Agent {
            return Err("Only an agent reply can acknowledge comments".into());
        }
        let through = self
            .thread(&post.thread_id)
            .and_then(|thread| {
                thread.messages.iter().find(|message| {
                    Some(&message.id) == post.message.in_reply_to.as_ref()
                        && message.author == Author::Reviewer
                })
            })
            .map(|message| message.sequence)
            .ok_or("in_reply_to must identify a reviewer comment in this thread")?;
        let thread = post.thread_id.clone();
        let id = self.post(post)?;
        let position = self.answered.entry(thread).or_default();
        *position = (*position).max(through);
        Ok(id)
    }

    pub fn retry(&self, thread: &ThreadId) -> Result<(), String> {
        let thread = self
            .thread(thread)
            .ok_or("The review thread no longer exists")?;
        if !self.pending(thread) {
            return Err("This thread has no unanswered, unresolved comments".into());
        }
        Ok(())
    }

    pub fn post(&mut self, post: Post) -> Result<MessageId, String> {
        if let Some(existing) = self.message(&post.message.id) {
            return if existing.same_content(&post.message)
                && self
                    .thread_for_message(&existing.id)
                    .map(|thread| &thread.id)
                    == Some(&post.thread_id)
            {
                Ok(existing.id.clone())
            } else {
                Err("This message identity is already used by a different post".into())
            };
        }
        if post.message.text.trim().is_empty() {
            return Err("A message cannot be empty".into());
        }
        self.append(post)
    }

    fn append(&mut self, mut post: Post) -> Result<MessageId, String> {
        let next = self.sequence.checked_add(1).ok_or("Too many messages")?;
        let id = post.message.id.clone();
        post.message.sequence = next;
        post.message.stamp_posting();
        let existing = self
            .threads
            .iter_mut()
            .find(|thread| thread.id == post.thread_id);
        if let Some(subject) = post.subject {
            match existing {
                Some(thread) if matches!(subject, ThreadSubject::Round { .. }) => {
                    if thread.subject != subject {
                        return Err("This review thread belongs to another subject".into());
                    }
                    thread.receive(post.message);
                }
                Some(_) => return Err("This review thread already exists".into()),
                None => self.threads.push(ReviewThread {
                    id: post.thread_id,
                    subject,
                    messages: vec![post.message],
                    resolution: Resolution::Open,
                    seen_reply_through: 0,
                    seen_replies: BTreeSet::new(),
                }),
            }
        } else {
            existing
                .ok_or("The review thread no longer exists")?
                .receive(post.message);
        }
        self.sequence = next;
        Ok(id)
    }
}

#[cfg(test)]
mod tests;
