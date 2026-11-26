use review_explore_page::{CommandRefusal, ThreadsFeed, ThreadsPublisher};
use review_threads::{
    AskedUnder, MessageId, Post, ReviewThreads, ThreadCommand, WakeupFailure, WakeupNotice,
};
use review_types::ReviewUnit;
use serde::Serialize;

use crate::sessions::Clock;

const REVIEW: &str = "standalone-review";

pub(crate) struct SessionThreads {
    threads: ReviewThreads,
    publisher: ThreadsPublisher,
    sent: Vec<SentMessage>,
    replies: usize,
    clock: Clock,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct SentMessage {
    round: String,
    text: String,
    asked_under: Option<AskedUnder>,
    quote: Option<String>,
}

impl SessionThreads {
    pub(crate) fn new(clock: Clock) -> Self {
        Self {
            threads: ReviewThreads::new(Self::review()),
            publisher: ThreadsPublisher::default(),
            sent: Vec::new(),
            replies: 0,
            clock,
        }
    }

    fn publish(&self) {
        self.publisher.loaded(self.clock.stamped(&self.threads));
    }

    pub(crate) fn review() -> ReviewUnit {
        REVIEW.into()
    }

    pub(crate) fn stamped(&self) -> ReviewThreads {
        self.clock.stamped(&self.threads)
    }

    pub(crate) fn subscribe(&self) -> ThreadsFeed {
        self.publisher.subscribe()
    }

    pub(crate) fn take(&mut self, command: ThreadCommand) -> Result<(), CommandRefusal> {
        let result = match command {
            ThreadCommand::Post { post, .. } => self.post(post),
            ThreadCommand::MarkRead {
                thread_id, through, ..
            } => self.threads.mark_read(&thread_id, through),
            ThreadCommand::Retry { thread_id, .. } => self.threads.retry(&thread_id).map(|()| {
                self.publisher.wakeup(Self::review(), None);
            }),
            _ => Ok(()),
        };
        self.publish();
        result.map_err(CommandRefusal::Failed)
    }

    fn post(&mut self, post: Post) -> Result<(), String> {
        let message = post.message().clone();
        let known = self.threads.message(&message.id).is_some();
        self.threads.post(post)?;
        if !known {
            let round = self
                .threads
                .thread_for_message(&message.id)
                .and_then(|thread| thread.round())
                .unwrap_or_default()
                .to_owned();
            self.sent.push(SentMessage {
                round,
                text: message.text,
                asked_under: message.asked_under,
                quote: message.quote,
            });
            self.publisher.wakeup(Self::review(), None);
        }
        Ok(())
    }

    pub(crate) fn agent_replies(&mut self, round: &str, text: &str) -> bool {
        let Some(thread) = self.threads.round_conversation(round) else {
            return false;
        };
        let Some(last) = thread.last_comment() else {
            return false;
        };
        self.replies += 1;
        let id = MessageId::parse(&format!("00000000-0000-4000-8000-{:012}", self.replies))
            .expect("a UUID");
        let post = Post::answer(thread.id.clone(), id, text.to_owned(), last.id.clone());
        if self.threads.answer(post).is_err() {
            return false;
        }
        self.publish();
        true
    }

    pub(crate) fn not_delivered(&mut self, error: &str) -> bool {
        let Some(through) = self.threads.pending_comment_sequence() else {
            return false;
        };
        let failure = WakeupFailure {
            through,
            error: error.to_owned(),
        };
        self.publisher
            .wakeup(Self::review(), Some(WakeupNotice::Failed(failure)));
        true
    }

    pub(crate) fn sent(&self) -> Vec<SentMessage> {
        self.sent.clone()
    }
}
