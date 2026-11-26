use std::io::Read;
use std::os::unix::net::UnixStream;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use agent_input_line::{Agent as InputAgent, Line};
use herdr_client::client::EventCanceller;
use herdr_client::protocol::{AgentPort, PaneId, PromptWait};
use herdr_client::{Error, Result};
use serde_json::json;

use super::screen_watch::{ScreenWatch, WatchCloser};
use super::{HcClient, SEND_TIMEOUT, Sent, parse, refusal, untyped};

const READ_GAP: Duration = Duration::from_millis(100);

enum Offered {
    Answered(Result<Sent>),
    Withdrawn(PromptWait),
    Cancelled,
}

enum Reader {
    Reads(InputAgent),
    Unread(String),
}

struct Offer<'a> {
    pane: &'a PaneId,
    text: &'a str,
    agent: &'a Reader,
    working: bool,
}

const CONFIRM_HELD: Duration = Duration::from_millis(150);

enum AfterPrint {
    Answered(Result<Sent>),
    Held(PromptWait),
    Typing,
    Withdrawn,
    Waits,
}

enum WhileOffered {
    Typing,
    Held(PromptWait),
    Empty,
}

enum Heard {
    Answer(Result<Sent>),
    Quiet,
    Printed,
    WatchEnded,
    Withdrawn,
}

impl HcClient {
    // herdr-core types a `terminal.send` once nobody pressed a key for about 3 s, whatever the
    // input line holds (hc 0.1.21): a person's pause mid-sentence gets the prompt typed after it.
    pub(super) fn send_on_empty_line(
        &self,
        pane: &PaneId,
        text: &str,
        waits: &mut dyn FnMut(Option<PromptWait>),
        withdrawal: &EventCanceller,
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
                let Some(_tie) = watch.tie(withdrawal)? else {
                    return Err(Error::Cancelled);
                };
                let _ = watch.printed();
                if withdrawal.is_cancelled() {
                    return Err(Error::Cancelled);
                }
                continue;
            }
            match self.offer(pane, text, &agent, &screen, watch, withdrawal)? {
                Offered::Answered(answer) => return answer,
                Offered::Withdrawn(wait) => tell(Some(wait)),
                Offered::Cancelled => return Err(Error::Cancelled),
            }
        }
    }

    fn input_agent(&self, pane: &PaneId) -> Result<Reader> {
        let agent = self.get_agent(pane)?.and_then(|agent| agent.agent);
        Ok(match agent.as_deref() {
            Some("claude") => Reader::Reads(InputAgent::ClaudeCode),
            Some("codex") => Reader::Reads(InputAgent::Codex),
            Some(other) => Reader::Unread(other.to_owned()),
            None => Reader::Unread("a program that reports no agent".to_owned()),
        })
    }

    fn offer(
        &self,
        pane: &PaneId,
        text: &str,
        agent: &Reader,
        screen: &str,
        mut watch: ScreenWatch,
        withdrawal: &EventCanceller,
    ) -> Result<Offered> {
        let socket = self.connect(Some(SEND_TIMEOUT))?;
        let hang_up = socket.try_clone().map_err(|source| Error::Io {
            operation: "share the herdr-core send socket",
            path: self.socket_path.clone(),
            source,
        })?;
        let io = |operation, source| Error::Io {
            operation,
            path: self.socket_path.clone(),
            source,
        };
        let (withdrawn, told) = UnixStream::pair()
            .map_err(|source| io("pair the sockets that tell a withdrawal", source))?;
        let tie = withdrawal
            .tie(&withdrawn)
            .map_err(|source| io("tie a send to its withdrawal", source))?;
        let Some(_tie) = tie else {
            return Err(Error::Cancelled);
        };
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
                // herdr-core answers a press of no key once the quiet pause that also holds a send
                // ends, but it does not wait for its hold behind unsent typing (`draft_wait`).
                let _ = self.request_on(
                    probe,
                    "terminal.keys",
                    &json!({"terminal": pane.0, "keys": []}),
                );
                let _ = quiet.send(Heard::Quiet);
            });
            let withdrawals = heard.clone();
            scope.spawn(move || {
                let _ = (&told).read(&mut [0]);
                let _ = withdrawals.send(Heard::Withdrawn);
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
            let _ = withdrawn.shutdown(std::net::Shutdown::Both);
            outcome
        })
    }

    // herdr-core types a send once its quiet pause ends, but answers it only once the agent went
    // working or a few seconds after it typed: a hang-up then loses the answer of a typed prompt.
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
            let sign = match heard {
                Heard::Answer(answer) => return Ok(Offered::Answered(answer)),
                Heard::Quiet => AfterPrint::Typing,
                _ if typing => continue,
                Heard::Withdrawn => AfterPrint::Withdrawn,
                Heard::Printed => self.after_print(offer, hearing, &mut last_read),
                Heard::WatchEnded => continue,
            };
            if let Some(offered) = self.act_on(sign, offer, hearing, hang_up, &mut typing) {
                return Ok(offered);
            }
        }
    }

    fn act_on(
        &self,
        sign: AfterPrint,
        offer: &Offer<'_>,
        hearing: &mpsc::Receiver<Heard>,
        hang_up: &UnixStream,
        typing: &mut bool,
    ) -> Option<Offered> {
        match sign {
            AfterPrint::Answered(answer) => Some(Offered::Answered(answer)),
            AfterPrint::Held(wait) => Some(withdraw(hearing, hang_up, wait)),
            AfterPrint::Withdrawn if !self.may_be_typing(offer) => Some(cancel(hearing, hang_up)),
            AfterPrint::Withdrawn | AfterPrint::Typing => {
                *typing = true;
                None
            }
            AfterPrint::Waits => None,
        }
    }

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
        if let Some(sign) = sign_among(hearing.try_iter()) {
            return sign;
        }
        match self.while_offered(offer) {
            WhileOffered::Typing => AfterPrint::Typing,
            WhileOffered::Held(_) => self.confirm_held(offer, hearing),
            WhileOffered::Empty => AfterPrint::Waits,
        }
    }

    // herdr-core ends its pause for a send and for a press of no key at once, but the screen the
    // agent draws as the prompt comes may reach the watch first, half drawn.
    fn confirm_held(&self, offer: &Offer<'_>, hearing: &mpsc::Receiver<Heard>) -> AfterPrint {
        let until = Instant::now() + CONFIRM_HELD;
        while let Some(left) = until.checked_duration_since(Instant::now()) {
            match hearing.recv_timeout(left) {
                Ok(Heard::Answer(answer)) => return AfterPrint::Answered(answer),
                Ok(Heard::Quiet) => return AfterPrint::Typing,
                Ok(Heard::Withdrawn) => return AfterPrint::Withdrawn,
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

    fn may_be_typing(&self, offer: &Offer<'_>) -> bool {
        matches!(self.while_offered(offer), WhileOffered::Typing)
    }

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

    // Without a `caller`, herdr-core types the text as it is, with no `<hc from="...">` tags.
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

fn sign_among(heard: impl IntoIterator<Item = Heard>) -> Option<AfterPrint> {
    let mut withdrawn = false;
    for heard in heard {
        match heard {
            Heard::Answer(answer) => return Some(AfterPrint::Answered(answer)),
            Heard::Quiet => return Some(AfterPrint::Typing),
            Heard::Withdrawn => withdrawn = true,
            Heard::Printed | Heard::WatchEnded => {}
        }
    }
    withdrawn.then_some(AfterPrint::Withdrawn)
}

fn answer_among(heard: impl IntoIterator<Item = Heard>) -> Option<Result<Sent>> {
    heard.into_iter().find_map(|heard| match heard {
        Heard::Answer(answer) => Some(answer),
        Heard::Quiet | Heard::Printed | Heard::WatchEnded | Heard::Withdrawn => None,
    })
}

// herdr-core 0.1.20 and later cancel a send whose client hangs up before it typed it.
fn withdraw(hearing: &mpsc::Receiver<Heard>, hang_up: &UnixStream, wait: PromptWait) -> Offered {
    let _ = hang_up.shutdown(std::net::Shutdown::Both);
    match answer_among(hearing.iter()) {
        Some(Ok(sent)) => Offered::Answered(Ok(sent)),
        Some(Err(_)) | None => Offered::Withdrawn(wait),
    }
}

fn cancel(hearing: &mpsc::Receiver<Heard>, hang_up: &UnixStream) -> Offered {
    let _ = hang_up.shutdown(std::net::Shutdown::Both);
    match answer_among(hearing.iter()) {
        Some(Ok(sent)) => Offered::Answered(Ok(sent)),
        Some(Err(_)) | None => Offered::Cancelled,
    }
}

// herdr-core pastes a whole prompt at once.
const PASTED: usize = 16;

// Claude Code shows a long paste as `[Pasted text #1 +12 lines]`, Codex as `[Pasted Content 1200
// chars]`, and both may wrap the text otherwise than it was written, within a word too.
fn is_pasted(shown: &str, text: &str) -> bool {
    let visible = |text: &str| text.split_whitespace().collect::<String>();
    let (shown, text) = (visible(shown), visible(text));
    shown.starts_with("[Pasted")
        || (text.starts_with(&shown)
            && (shown.chars().count() >= PASTED || shown.len() == text.len()))
}

// herdr-core paints the program's OSC 7501 reports with the screen it gives a watcher.
fn reports_working(screen: &str) -> bool {
    let Some((_, report)) = screen.rsplit_once("]7501;") else {
        return false;
    };
    let report = report.split(['', '']).next().unwrap_or_default();
    report.split(':').any(|field| field == "state=working")
}

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
