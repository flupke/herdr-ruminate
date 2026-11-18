//! A send that herdr-core holds behind typing the agent's input line does not show.
//!
//! herdr-core holds a `terminal.send` while it counts a person's unsent typing in the terminal
//! (herdr-core's `draft`): the characters they typed less their Backspaces, until they press Enter,
//! Ctrl-C or Ctrl-U, at most 60 s after their last key (hc 0.1.20 and later). Text cleared another
//! way, as with Claude Code's Esc Esc, Ctrl-W or Delete, or keys of its vim mode that type nothing,
//! leaves that count. On 11-18 a wakeup posted from a review thread waited so for 134 s, its agent
//! idle and its input line empty, and went 66 s after the user's own prompt there.
//!
//! So, while a send to an idle Claude Code waits, the client looks every few seconds whether nobody
//! may be typing in its terminal, and then presses Ctrl+U there through herdr-core, which ends the
//! count and erases nothing from an empty line: herdr-core then types the waiting send at once
//! (0.2 s, measured on hc 0.1.21). The send is never given up nor sent twice. The checks, each read
//! right before the press, in this order, are herdr-bowtie's (its ADR 0402): the agent shows idle,
//! the window's keyboard is not in its terminal (a window's view or layout that cannot be read may
//! have it there), its input line reads empty, and, last, nobody pressed a key in the terminal for
//! herdr-core's quiet pause, from any window or `hc attach`, which herdr-core tells by answering a
//! press of no key at once. When one fails, the send waits on, and the next look checks again. Only
//! a key typed in the moment between the last check and the press could make herdr-core hold the
//! Ctrl+U until its person pauses, and then erase what they typed.

use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use herdr_client::protocol::{Agent, AgentPort, PaneId};
use herdr_client::{Error, Result};
use serde_json::json;

use super::{HcClient, SOCKET_TIMEOUT, Sent};

/// How long a send waits before the client first looks whether unsent typing holds it, and
/// between two looks: past herdr-core's ~3 s quiet pause after a person's last key, and the
/// moment an idle Claude Code takes to show itself working, which the send waits for.
pub(super) const LOOK_EVERY: Duration = Duration::from_secs(5);

/// How long a press of no key may wait for its answer before it says someone typed in the
/// terminal within herdr-core's quiet pause: well past a request's round trip on this computer,
/// far short of that pause.
const QUIET_PROBE_TIMEOUT: Duration = Duration::from_millis(300);

impl HcClient {
    /// Sends `text` to `pane` with `send`, and while it waits, ends the unsent typing that
    /// herdr-core may hold it behind, once nobody may be typing there, as the module says.
    pub(super) fn send_past_unsent_typing(
        &self,
        pane: &PaneId,
        send: impl FnOnce() -> Result<Sent> + Send,
    ) -> Result<Sent> {
        thread::scope(|scope| {
            let (done, answer) = mpsc::channel();
            scope.spawn(move || {
                let _ = done.send(send());
            });
            loop {
                match answer.recv_timeout(self.look_every) {
                    Ok(result) => return result,
                    Err(RecvTimeoutError::Timeout) => {
                        if self.nobody_may_type(pane) {
                            // A key that fails leaves the send waiting, as before.
                            let _ = self.press(pane, &["ctrl+u"], SOCKET_TIMEOUT);
                        }
                    }
                    Err(RecvTimeoutError::Disconnected) => {
                        return Err(Error::Protocol {
                            operation: "terminal.send".to_owned(),
                            detail: "the send stopped before herdr-core answered",
                        });
                    }
                }
            }
        })
    }

    /// Whether nobody may be typing in `pane` now, each check read in the module's order, the
    /// one that nobody typed last; not when one cannot be read.
    fn nobody_may_type(&self, pane: &PaneId) -> bool {
        self.get_agent(pane)
            .is_ok_and(|agent| agent.as_ref().is_some_and(waits_as_claude_code))
            && self
                .keyboard_terminal()
                .is_ok_and(|focused| focused.as_deref() != Some(pane.0.as_str()))
            && self
                .screen(pane)
                .is_ok_and(|screen| claude_input_line::input_is_empty(&screen))
            && self.keys_quiet(pane)
    }

    /// The terminal that has the keyboard of the window the client follows, `None` for none; an
    /// error when the client follows no window, or cannot read its focus, which may then be in
    /// any terminal.
    fn keyboard_terminal(&self) -> Result<Option<String>> {
        let home = self.focus_home.as_ref().ok_or(Error::Protocol {
            operation: "read the window's focus".to_owned(),
            detail: "the client follows no window",
        })?;
        home.read_focused_terminal(&self.layout()?)
            .map_err(|error| Error::Herdr {
                operation: "read herdr-app's view",
                message: error.to_string(),
            })
    }

    /// The text of the screen `pane` shows.
    fn screen(&self, pane: &PaneId) -> Result<String> {
        #[derive(serde::Deserialize)]
        struct Read {
            text: String,
        }
        let read: Read = self.call("terminal.read", &json!({"terminal": pane.0}))?;
        Ok(read.text)
    }

    /// Whether nobody pressed a key in `pane` for herdr-core's quiet pause: it answers a press
    /// of no key at once then, and holds it while someone typed (measured on hc 0.1.20: 0.000 s
    /// quiet, 2.7 s after a person's key 0.3 s old).
    fn keys_quiet(&self, pane: &PaneId) -> bool {
        self.press(pane, &[], QUIET_PROBE_TIMEOUT).is_ok()
    }

    /// Presses `keys` in `pane` through herdr-core, which presses them once nobody typed there
    /// for its quiet pause, waiting `timeout` for its answer.
    fn press(&self, pane: &PaneId, keys: &[&str], timeout: Duration) -> Result<()> {
        match self.response(
            "terminal.keys",
            &json!({"terminal": pane.0, "keys": keys}),
            timeout,
        )? {
            Ok(_) => Ok(()),
            Err(error) => Err(Error::Herdr {
                operation: "terminal.keys",
                message: error.message,
            }),
        }
    }
}

/// Whether `agent` is a Claude Code agent that shows idle. Codex's composer is not read, so
/// nothing is pressed there.
fn waits_as_claude_code(agent: &Agent) -> bool {
    agent.agent.as_deref() == Some("claude") && agent.agent_status.waits_for_prompt()
}

#[cfg(test)]
#[path = "unsent_typing.tests.rs"]
mod tests;
