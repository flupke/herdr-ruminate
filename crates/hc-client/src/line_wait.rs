//! A prompt typed only while the agent's input line reads empty.
//!
//! herdr-core types a `terminal.send` once nobody pressed a key in the terminal for about 3 s,
//! whatever its line holds: with its hold behind unsent typing off (`draft_wait = 0`), a person who
//! pauses mid-sentence gets the prompt typed after their words (measured on hc 0.1.21: 3.1 s after
//! their last key). So the client reads the agent's input line right before it offers the send,
//! from the screen herdr-core paints to a watcher that attaches ([`ScreenWatch`]), styles included,
//! which tell a placeholder or a suggested prompt from typed text (crate `agent-input-line`). An
//! empty line gets the send. Text in the line, a screen with no input line, or an agent whose line
//! the client cannot read holds it, with no time limit: the client tells why, and reads the line
//! again each time the program prints, which the watch tells within a millisecond or so.
//!
//! The send it offers may still wait inside herdr-core, for the quiet pause after a key that
//! showed nothing, as one that emptied the line. So the client offers it with a press of no key,
//! which herdr-core answers when that same pause ends. While that press is unanswered, the watch
//! goes on: text in the line that is not the prompt herdr-core pasted, or no input line, still
//! there a moment later, means someone pressed a key within the pause, which herdr-core then still
//! waits out, and the client hangs up.
//! herdr-core cancels a waiting send whose client hung up (hc 0.1.20 and later): nothing was
//! typed, and the client reads the line again and offers the send again once it is empty. Once the
//! press is answered (herdr-core holds it while it types), the line shows the pasted prompt, or
//! the agent went working, herdr-core is typing the prompt, or typed it: the
//! client waits for its answer, which comes once the agent went working, and hangs up no more, so
//! that a key pressed after the prompt went never makes it go again. A send herdr-core answers was
//! typed, once. All this holds while herdr-core's hold behind unsent typing is off
//! (`draft_wait = 0`): with it on, the press of no key, which does not wait for that hold, could
//! be answered while herdr-core still holds the send.

use std::os::unix::net::UnixStream;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use agent_input_line::{Agent as InputAgent, Line};
use herdr_client::protocol::{AgentPort, PaneId, PromptWait};
use herdr_client::{Error, Result};
use serde_json::json;

use super::screen_watch::{ScreenWatch, WatchCloser};
use super::{HcClient, SEND_TIMEOUT, Sent, parse, refusal, untyped};

/// The least time between two reads of a waiting prompt's line: a program that prints all the
/// time, as a working agent's spinner, is read at most this often.
const READ_GAP: Duration = Duration::from_millis(100);

/// What became of a send the client offered.
enum Offered {
    /// herdr-core answered: it typed the prompt, or refused it.
    Answered(Result<Sent>),
    /// Someone typed, or opened a dialog, while herdr-core waited to type it: the client hung
    /// up, and herdr-core typed nothing. The prompt waits for that.
    Withdrawn(PromptWait),
}

/// How the client reads the input line of an agent: by the agent it is, or not at all for an
/// agent the reader knows nothing of, named as it reports itself.
enum Reader {
    Reads(InputAgent),
    Unread(String),
}

/// A send the client offered.
struct Offer<'a> {
    pane: &'a PaneId,
    text: &'a str,
    agent: &'a Reader,
    /// Whether the agent reported itself working when the client offered it.
    working: bool,
}

/// How long the client waits for a sign that herdr-core's pause ended before it takes a screen
/// that holds a send for one that does: far short of the pause of about 3 s that the key which
/// held it began.
const CONFIRM_HELD: Duration = Duration::from_millis(150);

/// What a print tells of a send the client offered.
enum AfterPrint {
    /// herdr-core answered the send.
    Answered(Result<Sent>),
    /// Someone pressed a key within herdr-core's pause: the client hangs up, and the prompt waits
    /// for this.
    Held(PromptWait),
    /// herdr-core may be typing the prompt: the client hangs up no more.
    Typing,
    /// The send waits on.
    Waits,
}

/// What the screen of an agent shows while a send to it waits.
enum WhileOffered {
    /// herdr-core is typing the prompt, or typed it: the line shows it pasted, or the agent went
    /// working.
    Typing,
    /// Someone typed in the line, or a dialog shows: the send must wait for that.
    Held(PromptWait),
    /// The line reads empty.
    Empty,
}

/// What the client hears while a send it offered waits.
enum Heard {
    Answer(Result<Sent>),
    /// herdr-core answered a press of no key, offered with the send: nobody typed in the
    /// terminal for its quiet pause, which also releases the send.
    Quiet,
    /// The program printed.
    Printed,
    /// The watch ended: only the answer may come.
    WatchEnded,
}

impl HcClient {
    /// Sends `text` to `pane` once its agent's input line reads empty, telling `waits` why it
    /// waits until then, as the module says.
    pub(super) fn send_on_empty_line(
        &self,
        pane: &PaneId,
        text: &str,
        waits: &mut dyn FnMut(Option<PromptWait>),
    ) -> Result<Sent> {
        let mut told: Option<PromptWait> = None;
        let mut tell = |wait: Option<PromptWait>| {
            if wait != told {
                waits(wait.clone());
                told = wait;
            }
        };
        let mut last_read: Option<Instant> = None;
        loop {
            if let Some(read) = last_read {
                thread::sleep(READ_GAP.saturating_sub(read.elapsed()));
            }
            last_read = Some(Instant::now());
            let (screen, mut watch) = ScreenWatch::open(&self.socket_path, pane)?;
            let agent = self.input_agent(pane)?;
            let wait = wait_for(&agent, &screen);
            let held = wait.is_some();
            tell(wait);
            if held {
                // A stream that ended, as with the agent's program, leaves the next watch to tell
                // why.
                let _ = watch.printed();
                continue;
            }
            match self.offer(pane, text, &agent, &screen, watch)? {
                Offered::Answered(answer) => return answer,
                Offered::Withdrawn(wait) => tell(Some(wait)),
            }
        }
    }

    /// The agent whose input line `pane` shows, by the name it reports.
    fn input_agent(&self, pane: &PaneId) -> Result<Reader> {
        let agent = self.get_agent(pane)?.and_then(|agent| agent.agent);
        Ok(match agent.as_deref() {
            Some("claude") => Reader::Reads(InputAgent::ClaudeCode),
            Some("codex") => Reader::Reads(InputAgent::Codex),
            Some(other) => Reader::Unread(other.to_owned()),
            None => Reader::Unread("a program that reports no agent".to_owned()),
        })
    }

    /// Offers the send of `text` to `pane`, whose terminal painted `screen` as the client read its line
    /// empty, and hangs up once `watch` shows someone typed or opened a dialog while herdr-core
    /// waits to type it.
    fn offer(
        &self,
        pane: &PaneId,
        text: &str,
        agent: &Reader,
        screen: &str,
        mut watch: ScreenWatch,
    ) -> Result<Offered> {
        let socket = self.connect(Some(SEND_TIMEOUT))?;
        let hang_up = socket.try_clone().map_err(|source| Error::Io {
            operation: "share the herdr-core send socket",
            path: self.socket_path.clone(),
            source,
        })?;
        let closer: WatchCloser = watch.closer()?;
        let probe = self.connect(Some(SEND_TIMEOUT))?;
        let probe_closer = probe.try_clone().map_err(|source| Error::Io {
            operation: "share the herdr-core probe socket",
            path: self.socket_path.clone(),
            source,
        })?;
        let (heard, hearing) = mpsc::channel();
        let offer = Offer {
            pane,
            text,
            agent,
            working: reports_working(screen),
        };
        thread::scope(|scope| {
            let answers = heard.clone();
            scope.spawn(move || {
                let _ = answers.send(Heard::Answer(self.send_on(socket, pane, text)));
            });
            let quiet = heard.clone();
            scope.spawn(move || {
                // Any answer ends the pause the client may hang up in: a refusal too, which
                // tells nothing of it.
                let _ = self.request_on(
                    probe,
                    "terminal.keys",
                    &json!({"terminal": pane.0, "keys": []}),
                );
                let _ = quiet.send(Heard::Quiet);
            });
            scope.spawn(move || {
                loop {
                    let printed = watch.printed();
                    let event = if printed.is_ok() {
                        Heard::Printed
                    } else {
                        Heard::WatchEnded
                    };
                    let ended = printed.is_err();
                    if heard.send(event).is_err() || ended {
                        return;
                    }
                }
            });
            let outcome = self.hear(&offer, &hearing, &hang_up);
            closer.close();
            let _ = probe_closer.shutdown(std::net::Shutdown::Both);
            outcome
        })
    }

    /// Hears what comes while the send of `offer` waits, until its answer. The client hangs up
    /// only while herdr-core waits out its quiet pause, which the press of no key offered with the
    /// send tells: herdr-core types the send once that pause ends, and answers it only once the
    /// agent went working, or a few seconds after it typed, so a hang-up then would lose the
    /// answer of a prompt typed already, which the client would offer again. A screen that shows
    /// the prompt in the line, or the agent gone working, ends that time too.
    fn hear(
        &self,
        offer: &Offer<'_>,
        hearing: &mpsc::Receiver<Heard>,
        hang_up: &UnixStream,
    ) -> Result<Offered> {
        let mut typing = false;
        let mut last_read: Option<Instant> = None;
        loop {
            let heard = hearing.recv().map_err(|_| Error::Protocol {
                operation: "terminal.send".to_owned(),
                detail: "the send stopped before herdr-core answered",
            })?;
            match heard {
                Heard::Answer(answer) => return Ok(Offered::Answered(answer)),
                Heard::Quiet => typing = true,
                Heard::Printed if !typing => {
                    match self.after_print(offer, hearing, &mut last_read) {
                        AfterPrint::Answered(answer) => return Ok(Offered::Answered(answer)),
                        AfterPrint::Held(wait) => return Ok(withdraw(hearing, hang_up, wait)),
                        AfterPrint::Typing => typing = true,
                        AfterPrint::Waits => {}
                    }
                }
                Heard::Printed | Heard::WatchEnded => {}
            }
        }
    }

    /// What a print tells of the send of `offer`, read at most every [`READ_GAP`] (`last_read`):
    /// one read for the prints that came meanwhile, unless an answer or the end of herdr-core's
    /// pause came among them.
    fn after_print(
        &self,
        offer: &Offer<'_>,
        hearing: &mpsc::Receiver<Heard>,
        last_read: &mut Option<Instant>,
    ) -> AfterPrint {
        if let Some(read) = *last_read {
            thread::sleep(READ_GAP.saturating_sub(read.elapsed()));
        }
        *last_read = Some(Instant::now());
        for heard in hearing.try_iter() {
            match heard {
                Heard::Answer(answer) => return AfterPrint::Answered(answer),
                Heard::Quiet => return AfterPrint::Typing,
                Heard::Printed | Heard::WatchEnded => {}
            }
        }
        match self.while_offered(offer) {
            WhileOffered::Typing => AfterPrint::Typing,
            WhileOffered::Held(_) => self.confirm_held(offer, hearing),
            WhileOffered::Empty => AfterPrint::Waits,
        }
    }

    /// Whether the screen of the agent `offer` sends to still holds the send after
    /// [`CONFIRM_HELD`], with no sign meanwhile that herdr-core's pause ended: herdr-core ends it
    /// for the send and for the press of no key at once, but the screen the program draws as the
    /// prompt comes may reach the client first, read half drawn.
    fn confirm_held(&self, offer: &Offer<'_>, hearing: &mpsc::Receiver<Heard>) -> AfterPrint {
        let until = Instant::now() + CONFIRM_HELD;
        while let Some(left) = until.checked_duration_since(Instant::now()) {
            match hearing.recv_timeout(left) {
                Ok(Heard::Answer(answer)) => return AfterPrint::Answered(answer),
                Ok(Heard::Quiet) => return AfterPrint::Typing,
                Ok(Heard::Printed | Heard::WatchEnded) => {}
                Err(_) => break,
            }
        }
        match self.while_offered(offer) {
            WhileOffered::Held(wait) => AfterPrint::Held(wait),
            WhileOffered::Typing => AfterPrint::Typing,
            WhileOffered::Empty => AfterPrint::Waits,
        }
    }

    /// What the screen of the agent `offer` sends to shows now. A screen that cannot be read
    /// tells nothing.
    fn while_offered(&self, offer: &Offer<'_>) -> WhileOffered {
        let Reader::Reads(agent) = offer.agent else {
            return WhileOffered::Empty;
        };
        let Ok((screen, _)) = ScreenWatch::open(&self.socket_path, offer.pane) else {
            return WhileOffered::Empty;
        };
        if !offer.working && reports_working(&screen) {
            return WhileOffered::Typing;
        }
        match agent_input_line::read(*agent, &screen) {
            Line::Typed(shown) if is_pasted(&shown, offer.text) => WhileOffered::Typing,
            Line::Typed(_) => WhileOffered::Held(PromptWait::TypedText),
            Line::Missing => WhileOffered::Held(PromptWait::NoInputLine),
            Line::Empty => WhileOffered::Empty,
        }
    }

    /// Sends `text` to `pane` on `socket`: herdr-core types it once nobody typed there for its
    /// quiet pause. The refusals `hc schema` lists typed nothing ([`untyped`]).
    fn send_on(&self, socket: UnixStream, pane: &PaneId, text: &str) -> Result<Sent> {
        match self.request_on(
            socket,
            "terminal.send",
            &json!({"terminal": pane.0, "text": text}),
        )? {
            Ok(result) => parse(result, "terminal.send"),
            Err(error) => match untyped(&error) {
                Some(reason) => Err(Error::NotTyped { reason }),
                None => Err(refusal("terminal.send", error)),
            },
        }
    }
}

/// The first answer among `heard`, if any.
fn answer_among(heard: impl IntoIterator<Item = Heard>) -> Option<Result<Sent>> {
    heard.into_iter().find_map(|heard| match heard {
        Heard::Answer(answer) => Some(answer),
        Heard::Quiet | Heard::Printed | Heard::WatchEnded => None,
    })
}

/// Hangs the send up on `hang_up`, which herdr-core takes for a withdrawal of a send it has not
/// typed, and waits for its outcome: an answer that came first stands, as herdr-core typed the
/// prompt then. The prompt waits for `wait` otherwise.
fn withdraw(hearing: &mpsc::Receiver<Heard>, hang_up: &UnixStream, wait: PromptWait) -> Offered {
    let _ = hang_up.shutdown(std::net::Shutdown::Both);
    match answer_among(hearing.iter()) {
        Some(Ok(sent)) => Offered::Answered(Ok(sent)),
        Some(Err(_)) | None => Offered::Withdrawn(wait),
    }
}

/// The least characters of a prompt, blanks aside, that an input line shows once herdr-core
/// pasted it: herdr-core pastes the whole prompt at once, and a person who types its first
/// characters, as every wakeup starts with the same words, types a few before a look sees them.
const PASTED: usize = 16;

/// Whether `shown`, the text an input line shows, is the prompt `text` herdr-core pasted: its
/// start, of [`PASTED`] characters at least or whole, or the agent's stand-in for a long paste,
/// Claude Code's `[Pasted text #1 +12 lines]` or Codex's `[Pasted Content 1200 chars]`. Both may
/// wrap the text otherwise than it was written, within a word too: only the characters that are
/// not blank are compared. A few characters that start the prompt may be a person's.
fn is_pasted(shown: &str, text: &str) -> bool {
    let visible = |text: &str| text.split_whitespace().collect::<String>();
    let (shown, text) = (visible(shown), visible(text));
    shown.starts_with("[Pasted")
        || (text.starts_with(&shown)
            && (shown.chars().count() >= PASTED || shown.len() == text.len()))
}

/// Whether the agent reports itself working on `screen`, by the last OSC 7501 state herdr-core
/// paints with it.
fn reports_working(screen: &str) -> bool {
    let Some((_, report)) = screen.rsplit_once("]7501;") else {
        return false;
    };
    let report = report.split(['', '']).next().unwrap_or_default();
    report.split(':').any(|field| field == "state=working")
}

/// Why a prompt to `agent`, whose terminal paints `screen`, waits; `None` when its input line
/// reads empty.
fn wait_for(agent: &Reader, screen: &str) -> Option<PromptWait> {
    let agent = match agent {
        Reader::Reads(agent) => *agent,
        Reader::Unread(name) => return Some(PromptWait::UnreadAgent(name.clone())),
    };
    match agent_input_line::read(agent, screen) {
        Line::Empty => None,
        Line::Typed(_) => Some(PromptWait::TypedText),
        Line::Missing => Some(PromptWait::NoInputLine),
    }
}

#[cfg(test)]
#[path = "line_wait.tests.rs"]
mod tests;
