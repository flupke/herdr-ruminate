//! Publishes the review comments the reviewer posts on herdr-bus, the machine's message bus, as
//! `herdr.words` messages on the output `words`, so that other programs, such as herdr-bowtie,
//! read what the reviewer wrote to an agent.
//!
//! A thread of its own writes the bus file: a bus that is locked, unwritable or missing never
//! holds up the posting or the agent's wakeup, and each failure goes to the reporter given at
//! start.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use herdr_bus::{Bus, Message, Output, Uuid};
use herdr_client::protocol::PaneId;
use herdr_words::Words;
use review_threads::{MessageId, ThreadId};
use review_types::ReviewUnit;
use serde::Serialize;

/// The output the reviewer publishes its review comments on, which other programs share for the
/// words written to an actor.
pub const OUTPUT: &str = "words";

/// The namespace of the bus `id` derived from a thread message's ID that is no UUID, which a
/// message saved by an older reviewer may hold: the same message keeps the same `id`.
const LEGACY_ID_NAMESPACE: Uuid = Uuid::from_u128(0x952e_692f_dcf1_4be3_bae8_115e_4bea_fa71);

/// A review comment the reviewer posted, as the bus carries it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PostedComment {
    /// The thread message's ID, the bus message's `id`: a retried post publishes once.
    pub id: MessageId,
    pub text: String,
    pub review_unit: ReviewUnit,
    pub thread: ThreadId,
    /// The Explore round, by its instance, when the thread is a round conversation.
    pub round: Option<String>,
    /// The pane of the agent the reviewer wakes for the comment, as Herdr gives its ID; `None`
    /// when it wakes none.
    pub to: Option<PaneId>,
}

/// The `data` of the message: the shared `herdr.words` fields, then where the comment was
/// written.
#[derive(Serialize)]
struct CommentWords<'a> {
    #[serde(flatten)]
    words: Words,
    review: &'a str,
    thread: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    round: Option<&'a str>,
}

impl PostedComment {
    fn message(&self) -> Message {
        let data = CommentWords {
            words: Words {
                author: herdr_words::USER.into(),
                to: self.to.as_ref().map(|pane| herdr_words::pane(&pane.0)),
                text: self.text.clone(),
            },
            review: self.review_unit.as_str(),
            thread: self.thread.as_str(),
            round: self.round.as_deref(),
        };
        let data = serde_json::to_value(data).expect("the words of a comment are JSON");
        let subject = self.text.lines().next().unwrap_or_default();
        Message::new(herdr_words::TYPE, data)
            .id(bus_id(&self.id))
            .subject(subject)
    }
}

/// The bus `id` of the thread message `id`: itself when it is a UUID, as the reviewer makes
/// them, else a `UUIDv5` of it.
fn bus_id(id: &MessageId) -> Uuid {
    Uuid::parse_str(id.as_str())
        .unwrap_or_else(|_| Uuid::new_v5(&LEGACY_ID_NAMESPACE, id.as_str().as_bytes()))
}

/// The thread that publishes the review comments on the bus file it was started on.
///
/// Dropping it waits until the comments handed to it so far are published, or failed, for at
/// most [`EXIT_WAIT`]: a locked bus file must not hold up the reviewer's exit.
pub struct CommentBus {
    sender: Option<Sender<PostedComment>>,
    thread: Option<JoinHandle<()>>,
    /// Disconnects once the thread ends.
    ended: Receiver<()>,
}

/// How long dropping the [`CommentBus`] waits for the comments it holds. herdr-bus waits up to 5
/// seconds on a locked file, for each of them.
const EXIT_WAIT: Duration = Duration::from_secs(1);

impl std::fmt::Debug for CommentBus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("CommentBus").finish_non_exhaustive()
    }
}

impl CommentBus {
    /// Opens the bus file at `path` and declares the output `words` on it, on a thread of its
    /// own; each failure, there or in a later publish, goes to `report`. A bus that failed is
    /// opened and declared again for the next comment.
    pub fn start(path: PathBuf, report: impl Fn(String) + Send + 'static) -> Self {
        let (sender, receiver) = mpsc::channel();
        let (end, ended) = mpsc::channel();
        let thread = thread::spawn(move || {
            let _end = end;
            Publisher { path, bus: None }.run(&receiver, &report);
        });
        Self {
            sender: Some(sender),
            thread: Some(thread),
            ended,
        }
    }

    /// Waits until every comment handed to it is published, or failed, however long the bus
    /// takes.
    pub fn close(mut self) {
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }

    /// The bus file of this machine: `$HERDR_BUS_DB`, else herdr-bus's own place.
    pub fn default_path() -> Result<PathBuf, String> {
        Bus::default_path().map_err(|error| error.to_string())
    }

    /// Publishes `comment` in the background.
    pub fn publish(&self, comment: PostedComment) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(comment);
        }
    }
}

impl Drop for CommentBus {
    fn drop(&mut self) {
        self.sender.take();
        let Some(thread) = self.thread.take() else {
            return;
        };
        // Past the wait, the thread goes on alone until the process exits.
        if self.ended.recv_timeout(EXIT_WAIT) == Err(RecvTimeoutError::Disconnected) {
            let _ = thread.join();
        }
    }
}

struct Publisher {
    path: PathBuf,
    /// The open bus, with `words` declared on it; `None` until it opens, and after a failure.
    bus: Option<Bus>,
}

impl Publisher {
    fn run(mut self, comments: &Receiver<PostedComment>, report: &dyn Fn(String)) {
        if let Err(error) = self.open() {
            report(format!(
                "Review comments will not reach herdr-bus until it opens: {error}"
            ));
        }
        for comment in comments {
            if let Err(error) = self.publish(&comment) {
                self.bus = None;
                report(error);
            }
        }
    }

    fn open(&mut self) -> Result<&Bus, String> {
        if self.bus.is_none() {
            let bus = Bus::open(&self.path).map_err(|error| error.to_string())?;
            let output = Output::new(OUTPUT)
                .describe("Words written to an actor, such as the reviewer's review comments")
                .types([herdr_words::TYPE]);
            bus.declare(&output).map_err(|error| error.to_string())?;
            self.bus = Some(bus);
        }
        Ok(self.bus.as_ref().expect("the bus was just opened"))
    }

    fn publish(&mut self, comment: &PostedComment) -> Result<(), String> {
        let message = comment.message();
        self.open()
            .and_then(|bus| {
                bus.publish(OUTPUT, &message)
                    .map_err(|error| error.to_string())
            })
            .map(|_| ())
            .map_err(|error| {
                format!("The comment is posted, but herdr-bus did not get it: {error}")
            })
    }
}

#[cfg(test)]
#[path = "lib.tests.rs"]
mod tests;
