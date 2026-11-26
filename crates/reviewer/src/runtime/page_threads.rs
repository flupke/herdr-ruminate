use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use review_explore_page::{
    CommandRefusal, CommandReply, ThreadSender, ThreadsFeed, ThreadsPublisher,
};
use review_thread_service::{self as comments, ThreadCommands};
use review_threads::{MessageId, ThreadCommand};

#[derive(Default)]
pub(super) struct PageThreads {
    publisher: ThreadsPublisher,
    posts: Mutex<HashMap<MessageId, CommandReply>>,
}

impl PageThreads {
    pub(super) fn subscribe(&self) -> ThreadsFeed {
        self.publisher.subscribe()
    }

    pub(super) fn observe(&self, event: &comments::Event) {
        match event {
            comments::Event::Loaded(loaded) => {
                if let Ok(threads) = &loaded.result {
                    self.publisher.loaded(threads.clone());
                }
            }
            comments::Event::Wakeup {
                review_unit,
                notice,
            } => self.publisher.wakeup(review_unit.clone(), notice.clone()),
            comments::Event::Posted(posted) => {
                if let Some(reply) = self.lock().remove(&posted.message_id) {
                    reply.send(posted.result.clone().map_err(CommandRefusal::Failed));
                }
            }
            comments::Event::RoundMessage(_) | comments::Event::Error(_) => {}
        }
    }

    pub(super) fn sender(self: &Arc<Self>, commands: ThreadCommands) -> ThreadSender {
        let threads = Arc::clone(self);
        ThreadSender::new(move |command, reply| {
            threads.take(command, reply, |command| commands.send(command));
        })
    }

    fn take(
        &self,
        command: ThreadCommand,
        reply: CommandReply,
        deliver: impl FnOnce(ThreadCommand),
    ) {
        match &command {
            ThreadCommand::Post { post, .. } => {
                self.lock().insert(post.message().id.clone(), reply);
            }
            _ => reply.send(Ok(())),
        }
        deliver(command);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<MessageId, CommandReply>> {
        self.posts.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
#[path = "page_threads.tests.rs"]
mod tests;
