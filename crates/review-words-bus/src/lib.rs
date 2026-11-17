//! Publishes on herdr-bus, the machine's message bus, the words the user writes to an agent in
//! the reviewer, as `herdr.words` messages on the output `words`, so that other programs, such
//! as herdr-bowtie, read them: the review comments, and the answers of an Explore round.
//!
//! A thread of its own writes the bus file: a bus that is locked, unwritable or missing never
//! holds up the posting or the agent's prompt, and each failure goes to the reporter given at
//! start.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use herdr_bus::{Bus, Message, Output, Uuid};
use herdr_client::protocol::PaneId;
use herdr_words::Words;
use review_threads::ThreadId;
use review_types::ReviewUnit;
use serde::Serialize;

/// The output the reviewer publishes the user's words on, which other programs share for the
/// words written to an actor.
pub const OUTPUT: &str = "words";

/// The namespace of the bus `id` derived from an ID that is no UUID, which a thread message
/// saved by an older reviewer may hold: the same message keeps the same `id`.
const LEGACY_ID_NAMESPACE: Uuid = Uuid::from_u128(0x952e_692f_dcf1_4be3_bae8_115e_4bea_fa71);

/// Words the user wrote to an agent in the reviewer, as the bus carries them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserWords {
    /// The ID the reviewer saved the words under, a thread message's or an answer's, the bus
    /// message's `id`: a retry publishes once.
    pub id: String,
    pub text: String,
    pub review_unit: ReviewUnit,
    pub place: Place,
    /// The pane of the agent the words go to, as Herdr gives its ID; `None` when they go to
    /// none.
    pub to: Option<PaneId>,
}

/// Where the user wrote the words, which goes on the bus beside them, named by its `place`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "place", rename_all = "snake_case")]
pub enum Place {
    /// A review comment in the thread `thread`, which is the conversation of the Explore round
    /// `round`, by its instance, when given.
    Thread {
        thread: ThreadId,
        #[serde(skip_serializing_if = "Option::is_none")]
        round: Option<String>,
    },
    /// An answer in the Explore round `round`, by its instance: the comment of an answer to the
    /// question `question`, by ID, with the choice `choice`, by ID, when it picks one, or a
    /// reply to the round's conclusion when it names no question. `drawing_board` when the
    /// answer sends the agent back to the drawing board, or replies to the conclusion it reached
    /// there.
    Answer {
        round: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        question: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        choice: Option<String>,
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        drawing_board: bool,
    },
    /// The lines the user wrote or changed in the list they sent with Implement, or Post review, from the conclusion of the turn
    /// `conclusion`, by its request, of the Explore round `round`, by its instance.
    Implementation { round: String, conclusion: String },
    /// The user's name, which they gave at the start of the Explore round `round`, by its
    /// instance, for the review of someone else's change.
    Start { round: String },
}

impl Place {
    /// What the reviewer did with the words, which a failure to publish them opens with.
    fn failure_prefix(&self) -> &'static str {
        match self {
            Self::Thread { .. } => "The comment is posted",
            Self::Answer { .. } => "The answer is sent",
            Self::Implementation { .. } => "The list is sent",
            Self::Start { .. } => "The round started",
        }
    }
}

/// The `data` of the message: the shared `herdr.words` fields, then where the user wrote them.
#[derive(Serialize)]
struct PlacedWords<'a> {
    #[serde(flatten)]
    words: Words,
    review: &'a str,
    #[serde(flatten)]
    place: &'a Place,
}

impl UserWords {
    fn message(&self) -> Message {
        let data = PlacedWords {
            words: Words {
                author: herdr_words::USER.into(),
                to: self.to.as_ref().map(|pane| herdr_words::pane(&pane.0)),
                text: self.text.clone(),
            },
            review: self.review_unit.as_str(),
            place: &self.place,
        };
        let data = serde_json::to_value(data).expect("the user's words are JSON");
        // The first line that says something: a list may start with an empty line.
        let subject = self
            .text
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or_default();
        Message::new(herdr_words::TYPE, data)
            .id(bus_id(&self.id))
            .subject(subject)
    }
}

/// The bus `id` of the words saved under `id`: itself when it is a UUID, as the reviewer makes
/// them, else a `UUIDv5` of it.
fn bus_id(id: &str) -> Uuid {
    Uuid::parse_str(id).unwrap_or_else(|_| Uuid::new_v5(&LEGACY_ID_NAMESPACE, id.as_bytes()))
}

/// The thread that publishes the user's words on the bus file it was started on.
///
/// Dropping it waits until the words handed to it so far are published, or failed, for at
/// most [`EXIT_WAIT`]: a locked bus file must not hold up the reviewer's exit.
pub struct WordsBus {
    sender: Option<Sender<Parcel>>,
    thread: Option<JoinHandle<()>>,
    /// Disconnects once the thread ends.
    ended: Receiver<()>,
}

/// How long dropping the [`WordsBus`] waits for the words it holds. herdr-bus waits up to 5
/// seconds on a locked file, for each of them.
const EXIT_WAIT: Duration = Duration::from_secs(1);

impl std::fmt::Debug for WordsBus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("WordsBus").finish_non_exhaustive()
    }
}

impl WordsBus {
    /// Opens the bus file at `path` and declares the output `words` on it, on a thread of its
    /// own; each failure, there or in a later publish, goes to `report`. A bus that failed is
    /// opened and declared again for the next words.
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

    /// Waits until all the words handed to it are published, or failed, however long the bus
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

    /// Publishes `words` in the background.
    pub fn publish(&self, words: UserWords) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(Parcel::Words(words));
        }
    }

    /// Waits until the words handed to it so far are published, or failed, however long the
    /// bus takes: a test learns that words it did not find on the bus by then were not
    /// published.
    #[cfg(any(test, feature = "flush"))]
    pub fn flush(&self) {
        let (done, flushed) = mpsc::channel();
        if let Some(sender) = &self.sender
            && sender.send(Parcel::Flush(done)).is_ok()
        {
            let _ = flushed.recv();
        }
    }
}

impl Drop for WordsBus {
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

/// What the publishing thread gets.
enum Parcel {
    Words(UserWords),
    /// Answer once the words before are published, or failed.
    #[cfg(any(test, feature = "flush"))]
    Flush(Sender<()>),
}

struct Publisher {
    path: PathBuf,
    /// The open bus, with `words` declared on it; `None` until it opens, and after a failure.
    bus: Option<Bus>,
}

impl Publisher {
    fn run(mut self, parcels: &Receiver<Parcel>, report: &dyn Fn(String)) {
        if let Err(error) = self.open() {
            report(format!(
                "What you write to agents will not reach herdr-bus until it opens: {error}"
            ));
        }
        for parcel in parcels {
            match parcel {
                Parcel::Words(words) => {
                    if let Err(error) = self.publish(&words) {
                        self.bus = None;
                        report(error);
                    }
                }
                #[cfg(any(test, feature = "flush"))]
                Parcel::Flush(done) => {
                    let _ = done.send(());
                }
            }
        }
    }

    fn open(&mut self) -> Result<&Bus, String> {
        if self.bus.is_none() {
            let bus = Bus::open(&self.path).map_err(|error| error.to_string())?;
            let output = Output::new(OUTPUT)
                .describe("Words written to an actor, such as what the user writes to an agent")
                .types([herdr_words::TYPE]);
            bus.declare(&output).map_err(|error| error.to_string())?;
            self.bus = Some(bus);
        }
        Ok(self.bus.as_ref().expect("the bus was just opened"))
    }

    fn publish(&mut self, words: &UserWords) -> Result<(), String> {
        let message = words.message();
        self.open()
            .and_then(|bus| {
                bus.publish(OUTPUT, &message)
                    .map_err(|error| error.to_string())
            })
            .map(|_| ())
            .map_err(|error| {
                format!(
                    "{}, but herdr-bus did not get it: {error}",
                    words.place.failure_prefix()
                )
            })
    }
}

#[cfg(test)]
#[path = "lib.tests.rs"]
mod tests;
