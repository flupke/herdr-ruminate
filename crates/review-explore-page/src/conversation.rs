use std::collections::HashMap;
use std::sync::Arc;

use review_round_conversation::RoundConversation;
use review_threads::{MessageId, ReviewThreads, ThreadCommand, WakeupNotice};
use review_types::ReviewUnit;
use tokio::sync::watch;

use crate::command::{CommandReply, await_reply};
use crate::notice::Problem;

#[derive(Clone, Debug, Default)]
pub(crate) struct ThreadsSnapshot {
    pub(crate) revision: u64,
    by_review: HashMap<ReviewUnit, Arc<ReviewThreads>>,
    notices: HashMap<ReviewUnit, WakeupNotice>,
}

impl ThreadsSnapshot {
    pub(crate) fn of(threads: Option<ReviewThreads>) -> Self {
        Self {
            revision: 0,
            by_review: threads
                .into_iter()
                .map(|threads| (threads.review_unit.clone(), Arc::new(threads)))
                .collect(),
            notices: HashMap::new(),
        }
    }

    pub(crate) fn threads(&self, unit: &ReviewUnit) -> Option<&ReviewThreads> {
        self.by_review.get(unit).map(Arc::as_ref)
    }

    pub(crate) fn holds(&self, unit: &ReviewUnit, id: &MessageId) -> bool {
        self.threads(unit)
            .is_some_and(|threads| threads.message(id).is_some())
    }

    pub(crate) fn conversation(&self, unit: &ReviewUnit, round: &str) -> RoundConversation {
        let notice = self.notices.get(unit);
        match self.threads(unit) {
            Some(threads) => RoundConversation::new(threads, round, notice),
            None => RoundConversation::new(&ReviewThreads::new(unit.clone()), round, notice),
        }
    }
}

pub struct ThreadsPublisher(watch::Sender<ThreadsSnapshot>);

impl ThreadsPublisher {
    pub fn loaded(&self, threads: ReviewThreads) {
        self.0.send_if_modified(|snapshot| {
            if snapshot.threads(&threads.review_unit) == Some(&threads) {
                return false;
            }
            snapshot.revision += 1;
            snapshot
                .by_review
                .insert(threads.review_unit.clone(), Arc::new(threads));
            true
        });
    }

    pub fn wakeup(&self, unit: ReviewUnit, notice: Option<WakeupNotice>) {
        self.0.send_if_modified(|snapshot| {
            if snapshot.notices.get(&unit) == notice.as_ref() {
                return false;
            }
            snapshot.revision += 1;
            match notice {
                Some(notice) => snapshot.notices.insert(unit, notice),
                None => snapshot.notices.remove(&unit),
            };
            true
        });
    }

    pub fn subscribe(&self) -> ThreadsFeed {
        ThreadsFeed(self.0.subscribe())
    }
}

impl Default for ThreadsPublisher {
    fn default() -> Self {
        Self(watch::Sender::new(ThreadsSnapshot::default()))
    }
}

#[derive(Clone, Debug)]
pub struct ThreadsFeed(watch::Receiver<ThreadsSnapshot>);

impl ThreadsFeed {
    pub(crate) fn latest(&self) -> ThreadsSnapshot {
        self.0.borrow().clone()
    }

    pub fn conversation(&self, unit: &ReviewUnit, round: &str) -> RoundConversation {
        self.0.borrow().conversation(unit, round)
    }

    pub(crate) async fn changed(&mut self) -> bool {
        self.0.changed().await.is_ok()
    }
}

#[derive(Clone)]
pub struct ThreadSender(Arc<dyn Fn(ThreadCommand, CommandReply) + Send + Sync>);

impl ThreadSender {
    pub fn new(deliver: impl Fn(ThreadCommand, CommandReply) + Send + Sync + 'static) -> Self {
        Self(Arc::new(deliver))
    }

    pub(crate) async fn send(&self, command: ThreadCommand) -> Result<bool, Problem> {
        let (reply, replied) = CommandReply::channel();
        (self.0)(command, reply);
        await_reply(replied).await
    }
}

#[derive(Clone)]
pub struct PageConversation {
    pub(crate) threads: ThreadsFeed,
    pub(crate) sender: ThreadSender,
}

impl PageConversation {
    pub fn new(threads: ThreadsFeed, sender: ThreadSender) -> Self {
        Self { threads, sender }
    }
}
