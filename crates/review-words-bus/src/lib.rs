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

pub const OUTPUT: &str = "words";

const LEGACY_ID_NAMESPACE: Uuid = Uuid::from_u128(0x952e_692f_dcf1_4be3_bae8_115e_4bea_fa71);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserWords {
    pub id: String,
    pub text: String,
    pub review_unit: ReviewUnit,
    pub place: Place,
    pub to: Option<PaneId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "place", rename_all = "snake_case")]
pub enum Place {
    Thread {
        thread: ThreadId,
        #[serde(skip_serializing_if = "Option::is_none")]
        round: Option<String>,
    },
    Answer {
        round: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        question: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        choice: Option<String>,
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        drawing_board: bool,
    },
    Implementation {
        round: String,
        conclusion: String,
    },
    Start {
        round: String,
    },
}

impl Place {
    fn failure_prefix(&self) -> &'static str {
        match self {
            Self::Thread { .. } => "The comment is posted",
            Self::Answer { .. } => "The answer is sent",
            Self::Implementation { .. } => "The list is sent",
            Self::Start { .. } => "The round started",
        }
    }
}

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

fn bus_id(id: &str) -> Uuid {
    Uuid::parse_str(id).unwrap_or_else(|_| Uuid::new_v5(&LEGACY_ID_NAMESPACE, id.as_bytes()))
}

pub struct WordsBus {
    sender: Option<Sender<Parcel>>,
    thread: Option<JoinHandle<()>>,
    ended: Receiver<()>,
}

// herdr-bus waits up to 5 seconds on a locked file, for each message.
const EXIT_WAIT: Duration = Duration::from_secs(1);

impl std::fmt::Debug for WordsBus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("WordsBus").finish_non_exhaustive()
    }
}

impl WordsBus {
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

    pub fn close(mut self) {
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }

    pub fn default_path() -> Result<PathBuf, String> {
        Bus::default_path().map_err(|error| error.to_string())
    }

    pub fn publish(&self, words: UserWords) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(Parcel::Words(words));
        }
    }

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
        if self.ended.recv_timeout(EXIT_WAIT) == Err(RecvTimeoutError::Disconnected) {
            let _ = thread.join();
        }
    }
}

enum Parcel {
    Words(UserWords),
    #[cfg(any(test, feature = "flush"))]
    Flush(Sender<()>),
}

struct Publisher {
    path: PathBuf,
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
