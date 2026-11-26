use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    Message, MessageId, Post, ReviewThread, ReviewThreads, ThreadId, ThreadSource, ThreadSubject,
};

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum DraftTarget {
    File(String),
    Thread(ThreadId),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Draft {
    pub target: DraftTarget,
    pub source: Arc<ThreadSource>,
    pub reply_to: Option<MessageId>,
    pub text: String,
    id: MessageId,
    thread: ThreadId,
}

impl Draft {
    pub fn message_id(&self) -> &MessageId {
        &self.id
    }

    pub fn is_new_thread(&self) -> bool {
        matches!(self.target, DraftTarget::File(_))
    }

    pub fn thread_id(&self) -> &ThreadId {
        &self.thread
    }

    pub fn renew_publication(&mut self) {
        self.id = Message::reviewer(String::new()).id;
        if self.is_new_thread() {
            self.thread = ThreadId(uuid::Uuid::new_v4().to_string());
        }
    }

    pub fn start(path: String, source: Arc<ThreadSource>) -> Self {
        Self {
            target: DraftTarget::File(path),
            source,
            reply_to: None,
            text: String::new(),
            id: Message::reviewer(String::new()).id,
            thread: ThreadId(uuid::Uuid::new_v4().to_string()),
        }
    }

    pub fn reply(thread: &ReviewThread, reply_to: MessageId) -> Option<Self> {
        let ThreadSubject::Code(source) = &thread.subject else {
            return None;
        };
        Some(Self {
            target: DraftTarget::Thread(thread.id.clone()),
            source: source.clone(),
            reply_to: Some(reply_to),
            text: String::new(),
            id: Message::reviewer(String::new()).id,
            thread: thread.id.clone(),
        })
    }

    pub fn path(&self) -> &str {
        match &self.target {
            DraftTarget::File(path) => path,
            DraftTarget::Thread(_) => self
                .source
                .anchor
                .new_path
                .as_deref()
                .or(self.source.anchor.old_path.as_deref())
                .unwrap_or(""),
        }
    }

    pub fn post(&self) -> Post {
        let mut message = Message::reviewer(self.text.clone());
        message.id = self.id.clone();
        Post {
            thread_id: self.thread.clone(),
            message,
            subject: self
                .is_new_thread()
                .then(|| ThreadSubject::Code(self.source.clone())),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct SavedDrafts {
    drafts: Vec<Draft>,
}

impl SavedDrafts {
    pub fn drafts(&self) -> &[Draft] {
        &self.drafts
    }

    pub fn save(&mut self, draft: Draft, threads: &ReviewThreads) -> Result<(), String> {
        if let DraftTarget::Thread(id) = &draft.target
            && threads.thread(id).is_none()
        {
            return Err("The draft's review thread no longer exists".into());
        }
        if threads.message(&draft.id).is_some() {
            return Err("This draft has already been posted".into());
        }
        self.keep(draft);
        Ok(())
    }

    pub fn keep(&mut self, draft: Draft) {
        self.discard(&draft.thread);
        self.drafts.push(draft);
    }

    pub fn merge(&mut self, other: Self) {
        for draft in other {
            if !self.drafts.iter().any(|kept| kept.thread == draft.thread) {
                self.drafts.push(draft);
            }
        }
    }

    pub fn discard(&mut self, thread: &ThreadId) {
        self.drafts.retain(|draft| &draft.thread != thread);
    }

    pub fn forget_posted(&mut self, threads: &ReviewThreads) {
        self.drafts
            .retain(|draft| threads.message(&draft.id).is_none());
    }

    pub fn is_empty(&self) -> bool {
        self.drafts.is_empty()
    }

    pub fn sources_mut(&mut self) -> impl Iterator<Item = &mut Arc<ThreadSource>> {
        self.drafts.iter_mut().map(|draft| &mut draft.source)
    }
}

impl IntoIterator for SavedDrafts {
    type Item = Draft;
    type IntoIter = std::vec::IntoIter<Draft>;

    fn into_iter(self) -> Self::IntoIter {
        self.drafts.into_iter()
    }
}
