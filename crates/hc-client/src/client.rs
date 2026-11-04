//! herdr-core's JSON-RPC socket, one message per line.

use std::collections::BTreeMap;
use std::io::BufReader;
use std::net::Shutdown;
use std::ops::ControlFlow;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use herdr_app_focus::{AppHome, LAYOUT_DOCUMENT, Layout, ViewWatch};

use herdr_client::client::{EventCanceller, Tie, read_line, write_json_line};
use herdr_client::protocol::{
    Agent, AgentHost, AgentPort, AgentStatus, HerdrEvent, PaneId, PaneProcessInfo, SessionReport,
    SessionSnapshot, TabId, WorkspaceId,
};
use herdr_client::{Error, Result};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::events::{Event, Roster, StatusChange, status_of};
use crate::focus::Focus;
use crate::terminal::{SESSION_LABEL, Terminal, place_of, session_label};

const SOCKET_TIMEOUT: Duration = Duration::from_secs(5);
/// How long `terminal.send` may take: it waits until nobody typed in the terminal for about
/// 3 seconds, then a few seconds more to tell whether the program went working.
const SEND_TIMEOUT: Duration = Duration::from_secs(60);
static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// A synchronous client of herdr-core, the core of herdr-app that runs its terminals and
/// agents. herdr-core keeps no focus and no agent session: the reviewer keeps the agent's
/// session in its terminal's session label, which the agent's hook and
/// [`AgentHost::report_agent_session`] set (ADR 0016), and the client reads the focus of a
/// screen of herdr-app when it follows one.
#[derive(Clone, Debug)]
pub struct HcClient {
    socket_path: PathBuf,
    /// The home of the herdr-app screen whose focus the client follows.
    focus_home: Option<AppHome>,
}

/// A refusal of herdr-core: `{code, message, data: {kind, hint}}`.
#[derive(Debug, Deserialize)]
struct ApiError {
    message: String,
    #[serde(default)]
    data: Option<ErrorData>,
}

#[derive(Debug, Deserialize)]
struct ErrorData {
    /// Such as `not_found`, `blocked` or `finished`.
    #[serde(default)]
    kind: Option<String>,
}

impl ApiError {
    fn is(&self, kind: &str) -> bool {
        self.data
            .as_ref()
            .and_then(|data| data.kind.as_deref())
            .is_some_and(|actual| actual == kind)
    }
}

/// One line from herdr-core: the answer to a request, or a notification.
#[derive(Debug, Deserialize)]
struct Message {
    #[serde(default)]
    id: Option<Value>,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<ApiError>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    params: Option<Value>,
}

/// What `terminal.send` answers (`SendResult` in `hc schema`).
#[derive(Debug, Deserialize)]
struct Sent {
    /// Whether the program went working within a few seconds; `None` when it reports no
    /// status.
    #[serde(default)]
    started: Option<bool>,
    /// The program's status when the text went in.
    #[serde(default)]
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Listed {
    terminals: Vec<Terminal>,
}

/// What `doc.get` answers for herdr-app's layout: `null` before herdr-app saved one.
#[derive(Debug, Deserialize)]
struct Document {
    #[serde(default)]
    value: Option<Layout>,
}

/// The snapshot `events.subscribe` answers, before the events that follow it.
#[derive(Debug, Deserialize)]
struct Snapshot {
    terminals: Vec<Terminal>,
}

/// The events of one subscription, in the order herdr-core sent them.
struct Subscription {
    reader: BufReader<UnixStream>,
    /// Ties the stream to what ends it: a read it ends is no error.
    tie: Tie,
    socket_path: PathBuf,
}

/// What [`HcClient::forward_events`] waits for: herdr-core's next event, read on a thread of its
/// own, or herdr-app writing its view.
enum Input {
    /// The next event; `None` once the stream's canceller ended it.
    Event(Result<Option<Event>>),
    ViewWritten,
}

/// Ends the reads of a subscription's thread when it drops, by shutting its socket down.
struct Reader(UnixStream);

impl Drop for Reader {
    fn drop(&mut self) {
        let _ = self.0.shutdown(Shutdown::Both);
    }
}

/// Tells `inputs` each time herdr-app writes the view of the screen whose home is `home`, until
/// the watch drops. A home that cannot be watched leaves the focus as first read: agent
/// delivery does without the focus, as with only one agent in the tab, not without the events.
fn watch_view(home: &AppHome, inputs: mpsc::Sender<Input>) -> Option<ViewWatch> {
    home.watch_view(move || {
        let _ = inputs.send(Input::ViewWritten);
    })
    .ok()
}

impl HcClient {
    /// A client of the core whose socket is `socket_path`.
    pub fn new(socket_path: PathBuf) -> Self {
        Self {
            socket_path,
            focus_home: None,
        }
    }

    /// The client, following the focus of the herdr-app screen whose home is `home`: its
    /// session snapshot gives the focused terminal, and its events each terminal the user
    /// focuses, as Herdr's `pane.focused` tells.
    #[must_use]
    pub fn following_focus(self, home: AppHome) -> Self {
        Self {
            focus_home: Some(home),
            ..self
        }
    }

    /// A client of the core of the terminal this process runs in: herdr-core gives each of its
    /// terminals its socket in `HC_TERMINAL_SOCKET`.
    pub fn from_env() -> Result<Self> {
        std::env::var_os("HC_TERMINAL_SOCKET")
            .filter(|path| !path.is_empty())
            .map(|path| Self::new(path.into()))
            .ok_or(Error::Environment {
                name: "HC_TERMINAL_SOCKET",
            })
    }

    /// The labels of `terminal`.
    pub fn labels(&self, terminal: &PaneId) -> Result<BTreeMap<String, String>> {
        let terminal = self.terminal(terminal)?.ok_or_else(|| Error::Herdr {
            operation: "terminal.get",
            message: format!("no terminal {:?}", terminal.0),
        })?;
        Ok(terminal.labels)
    }

    /// The space and the tab of herdr-app that `terminal` shows in, by its `herdr.space` and
    /// `herdr.tab` labels; `None` for a terminal herdr-app does not show.
    pub fn place(&self, terminal: &PaneId) -> Result<Option<(WorkspaceId, TabId)>> {
        Ok(place_of(&self.labels(terminal)?))
    }

    /// Sets `labels` on `terminal`; a label set to an empty value is removed. herdr-core tells
    /// each subscriber of the terminal's events.
    pub fn set_labels(&self, terminal: &PaneId, labels: &BTreeMap<String, String>) -> Result<()> {
        self.call::<Value>(
            "terminal.label",
            &json!({"terminal": terminal.0, "labels": labels}),
        )
        .map(|_| ())
    }

    /// Names `session` as the session that the agent `agent` of `terminal` runs, in the
    /// terminal's session label, which herdr-core tells each subscriber of the terminal's events.
    pub fn report_session(&self, terminal: &PaneId, agent: &str, session: &str) -> Result<()> {
        self.set_labels(
            terminal,
            &BTreeMap::from([(SESSION_LABEL.to_owned(), session_label(agent, session))]),
        )
    }

    /// Removes the session label of `terminal` if it still names `session` of the agent
    /// `agent`, which ended: one a later session of the terminal set stays.
    pub fn forget_session(&self, terminal: &PaneId, agent: &str, session: &str) -> Result<()> {
        let labels = self.labels(terminal)?;
        if labels.get(SESSION_LABEL) != Some(&session_label(agent, session)) {
            return Ok(());
        }
        self.set_labels(
            terminal,
            &BTreeMap::from([(SESSION_LABEL.to_owned(), String::new())]),
        )
    }

    /// What `input` tells the reviewer: the arrival or departure of an agent, or the terminal
    /// the user focuses when it changed, if any; `Break` once the stream ended.
    fn tell(
        &self,
        input: Input,
        roster: &mut Roster,
        focus: Option<&mut Focus>,
    ) -> Result<ControlFlow<(), Option<HerdrEvent>>> {
        let event = match input {
            Input::ViewWritten => {
                return Ok(ControlFlow::Continue(
                    focus.and_then(Focus::changed).map(HerdrEvent::PaneFocused),
                ));
            }
            Input::Event(event) => event?,
        };
        let Some(event) = event else {
            return Ok(ControlFlow::Break(()));
        };
        match focus {
            Some(focus) if event.saved_document() == Some(LAYOUT_DOCUMENT) => {
                Ok(ControlFlow::Continue(
                    // A layout that cannot be read keeps the last one read.
                    self.layout()
                        .ok()
                        .and_then(|layout| focus.layout_changed(layout))
                        .map(HerdrEvent::PaneFocused),
                ))
            }
            _ => roster.observe(&event).map(ControlFlow::Continue),
        }
    }

    /// herdr-app's layout, which herdr-core keeps for it; empty before herdr-app saved one.
    fn layout(&self) -> Result<Layout> {
        let document: Document = self.call("doc.get", &json!({"name": LAYOUT_DOCUMENT}))?;
        Ok(document.value.unwrap_or_default())
    }

    /// herdr-app's layout, or none when it cannot be read: the agents and their prompts do
    /// without the focus, which then falls on no terminal.
    fn layout_or_none(&self) -> Layout {
        self.layout().unwrap_or_default()
    }

    /// The terminal at `address`, live or finished; `None` when herdr-core has none there.
    fn terminal(&self, address: &PaneId) -> Result<Option<Terminal>> {
        match self.response(
            "terminal.get",
            &json!({"terminal": address.0}),
            SOCKET_TIMEOUT,
        )? {
            Ok(result) => parse(result, "terminal.get").map(Some),
            Err(error) if error.is("not_found") => Ok(None),
            Err(error) => Err(refusal("terminal.get", error)),
        }
    }

    /// Pastes `text` into the terminal of `pane_id` and presses Enter. Sent without a caller,
    /// so herdr-core types the text as it is, without the `<hc from="...">` tags it puts around
    /// what one of its terminals sends another.
    fn send(&self, pane_id: &PaneId, text: &str) -> Result<Sent> {
        match self.response(
            "terminal.send",
            &json!({"terminal": pane_id.0, "text": text}),
            SEND_TIMEOUT,
        )? {
            Ok(result) => parse(result, "terminal.send"),
            Err(error) => Err(refusal("terminal.send", error)),
        }
    }

    /// Calls `method` with `params`, and parses its result; herdr-core's refusal is an error.
    fn call<T: DeserializeOwned>(&self, method: &'static str, params: &Value) -> Result<T> {
        match self.response(method, params, SOCKET_TIMEOUT)? {
            Ok(result) => parse(result, method),
            Err(error) => Err(refusal(method, error)),
        }
    }

    fn response(
        &self,
        method: &'static str,
        params: &Value,
        timeout: Duration,
    ) -> Result<std::result::Result<Value, ApiError>> {
        let mut socket = self.connect(Some(timeout))?;
        let id = REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let request = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        write_json_line(&mut socket, &request, method, &self.socket_path)?;
        let mut reader = BufReader::new(socket);
        loop {
            let line = read_line(&mut reader, method, &self.socket_path)?;
            if line.is_empty() {
                return Err(Error::Protocol {
                    operation: method.to_owned(),
                    detail: "herdr-core closed the connection before it answered",
                });
            }
            let message: Message = serde_json::from_slice(&line).map_err(|source| Error::Json {
                operation: method,
                source,
            })?;
            if message.id == Some(json!(id)) {
                return answer(message, method);
            }
        }
    }

    fn connect(&self, timeout: Option<Duration>) -> Result<UnixStream> {
        let socket = UnixStream::connect(&self.socket_path).map_err(|source| Error::Io {
            operation: "connect to herdr-core",
            path: self.socket_path.clone(),
            source,
        })?;
        socket
            .set_read_timeout(timeout)
            .and_then(|()| socket.set_write_timeout(timeout))
            .map_err(|source| Error::Io {
                operation: "configure the herdr-core socket",
                path: self.socket_path.clone(),
                source,
            })?;
        Ok(socket)
    }

    /// Subscribes to the events of the terminals `params` match, and returns the snapshot of
    /// those terminals with the events that follow it; `None` when `params` name a terminal
    /// herdr-core does not know. `canceller` ends the subscription.
    fn subscribe(
        &self,
        params: &Value,
        canceller: &EventCanceller,
    ) -> Result<Option<(Vec<Terminal>, Subscription)>> {
        const OPERATION: &str = "events.subscribe";
        let mut socket = self.connect(None)?;
        let tie = canceller
            .tie(&socket)
            .map_err(|source| Error::Io {
                operation: "configure herdr-core event cancellation",
                path: self.socket_path.clone(),
                source,
            })?
            .ok_or(Error::Cancelled)?;
        let id = REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let request = json!({"jsonrpc": "2.0", "id": id, "method": OPERATION, "params": params});
        let or_cancellation = |error| {
            if canceller.is_cancelled() {
                Error::Cancelled
            } else {
                error
            }
        };
        write_json_line(&mut socket, &request, OPERATION, &self.socket_path)
            .map_err(or_cancellation)?;
        let mut subscription = Subscription {
            reader: BufReader::new(socket),
            tie,
            socket_path: self.socket_path.clone(),
        };
        let message = subscription
            .next_message(OPERATION)?
            .ok_or(Error::Cancelled)?;
        if message.id != Some(json!(id)) {
            return Err(Error::Protocol {
                operation: OPERATION.to_owned(),
                detail: "herdr-core sent something else before it answered the subscription",
            });
        }
        match answer::<Snapshot>(message, OPERATION)? {
            Ok(snapshot) => Ok(Some((snapshot.terminals, subscription))),
            Err(error) if error.is("not_found") => Ok(None),
            Err(error) => Err(refusal(OPERATION, error)),
        }
    }
}

impl Subscription {
    /// Reads the subscription's events on a thread of its own, which hands each to `inputs`
    /// until the stream ends, the receiver drops, or the [`Reader`] it returns drops.
    fn forward_to(mut self, inputs: mpsc::Sender<Input>) -> Result<Reader> {
        let socket = self
            .reader
            .get_ref()
            .try_clone()
            .map_err(|source| Error::Io {
                operation: "share the herdr-core event socket",
                path: self.socket_path.clone(),
                source,
            })?;
        thread::spawn(move || {
            loop {
                let event = self.next_event();
                let ended = !matches!(event, Ok(Some(_)));
                if inputs.send(Input::Event(event)).is_err() || ended {
                    return;
                }
            }
        });
        Ok(Reader(socket))
    }

    /// The next event; `None` once the stream's canceller ended it. A subscriber that fell
    /// behind is an error: it subscribes again for a fresh snapshot.
    fn next_event(&mut self) -> Result<Option<Event>> {
        const OPERATION: &str = "read herdr-core events";
        while let Some(message) = self.next_message(OPERATION)? {
            match message.method.as_deref() {
                Some("event") => {
                    let params = message.params.unwrap_or_default();
                    return Event::deserialize(params)
                        .map(Some)
                        .map_err(|source| Error::Json {
                            operation: OPERATION,
                            source,
                        });
                }
                Some("missed") => {
                    return Err(Error::Protocol {
                        operation: OPERATION.to_owned(),
                        detail: "the subscriber fell behind herdr-core's events",
                    });
                }
                _ => {}
            }
        }
        Ok(None)
    }

    /// The next line herdr-core sends; `None` once the canceller ended the stream.
    fn next_message(&mut self, operation: &'static str) -> Result<Option<Message>> {
        let line = match read_line(&mut self.reader, operation, &self.socket_path) {
            Ok(line) => line,
            Err(_) if self.tie.is_cancelled() => return Ok(None),
            Err(error) => return Err(error),
        };
        if line.is_empty() {
            if self.tie.is_cancelled() {
                return Ok(None);
            }
            return Err(Error::Protocol {
                operation: operation.to_owned(),
                detail: "herdr-core closed the event stream",
            });
        }
        serde_json::from_slice(&line)
            .map(Some)
            .map_err(|source| Error::Json { operation, source })
    }
}

/// The result of an answer, or herdr-core's refusal.
fn answer<T: DeserializeOwned>(
    message: Message,
    method: &'static str,
) -> Result<std::result::Result<T, ApiError>> {
    match (message.result, message.error) {
        (Some(result), None) => parse(result, method).map(Ok),
        (None, Some(error)) => Ok(Err(error)),
        _ => Err(Error::Protocol {
            operation: method.to_owned(),
            detail: "herdr-core answered with neither one result nor one error",
        }),
    }
}

fn parse<T: DeserializeOwned>(value: Value, method: &'static str) -> Result<T> {
    serde_json::from_value(value).map_err(|source| Error::Json {
        operation: method,
        source,
    })
}

fn refusal(method: &'static str, error: ApiError) -> Error {
    Error::Herdr {
        operation: method,
        message: error.message,
    }
}

impl AgentPort for HcClient {
    /// herdr-core keeps no focus: herdr-app keeps it on each screen, and gives it only to a
    /// client that follows the screen's.
    fn session_snapshot(&self) -> Result<SessionSnapshot> {
        let focused_pane_id = match &self.focus_home {
            Some(home) => home.focused_terminal(&self.layout_or_none()).map(PaneId),
            None => None,
        };
        Ok(SessionSnapshot { focused_pane_id })
    }

    fn list_agents(&self) -> Result<Vec<Agent>> {
        let listed: Listed = self.call("terminal.list", &json!({}))?;
        Ok(listed
            .terminals
            .iter()
            .filter_map(Terminal::agent)
            .collect())
    }

    fn get_agent(&self, pane_id: &PaneId) -> Result<Option<Agent>> {
        Ok(self.terminal(pane_id)?.as_ref().and_then(Terminal::agent))
    }

    fn pane_process_info(&self, pane_id: &PaneId) -> Result<PaneProcessInfo> {
        let terminal: Terminal = self.call("terminal.get", &json!({"terminal": pane_id.0}))?;
        Ok(terminal.process_info())
    }

    /// herdr-core tells whether the agent went working within a few seconds; an agent that
    /// blocked on its user at once started too, as Herdr's wait judges it.
    fn prompt_agent(&self, pane_id: &PaneId, text: &str) -> Result<()> {
        let sent = self.send(pane_id, text)?;
        if sent.started != Some(false) {
            return Ok(());
        }
        let started = self
            .get_agent(pane_id)?
            .is_some_and(|agent| agent.agent_status.started_on_prompt());
        if started {
            return Ok(());
        }
        Err(Error::AgentNotStarted {
            message: format!(
                "the agent stayed {} after the prompt",
                sent.status.as_deref().unwrap_or("silent")
            ),
        })
    }

    fn submit_prompt(&self, pane_id: &PaneId, text: &str) -> Result<()> {
        self.send(pane_id, text).map(|_| ())
    }

    /// herdr-core tells each change of a terminal's labels, the session label's included.
    fn announces_sessions(&self) -> bool {
        true
    }
}

impl AgentHost for HcClient {
    /// The agents' arrivals and departures, and, for a client that follows a screen's focus,
    /// each terminal the user focuses there, the one focused at first included.
    fn forward_events(
        &self,
        canceller: &EventCanceller,
        send: &mut dyn FnMut(HerdrEvent) -> bool,
    ) -> Result<()> {
        let Some((snapshot, events)) = self.subscribe(&json!({}), canceller)? else {
            return Err(Error::Protocol {
                operation: "events.subscribe".to_owned(),
                detail: "herdr-core found no terminal for a subscription that names none",
            });
        };
        let mut roster = Roster::new(snapshot);
        // A session reported, or an agent gone, while no subscription held, as when this one
        // follows a stream that dropped, reaches no one otherwise: whoever waits on an agent
        // looks again.
        for agent in roster.agents().collect::<Vec<_>>() {
            if !send(HerdrEvent::AgentSessionChanged(agent)) {
                return Ok(());
            }
        }
        let (inputs, received) = mpsc::channel();
        let _reader = events.forward_to(inputs.clone())?;
        // The watch ends when it drops, with the stream.
        let (mut focus, _view_watch) = match &self.focus_home {
            Some(home) => (
                Some(Focus::new(home.clone(), self.layout_or_none())),
                watch_view(home, inputs),
            ),
            None => (None, None),
        };
        let focused = focus.as_mut().and_then(Focus::changed);
        if focused.is_some_and(|terminal| !send(HerdrEvent::PaneFocused(terminal))) {
            return Ok(());
        }
        // The reader's last input, the end of its stream or an error, ends the loop.
        while let Ok(input) = received.recv() {
            let ControlFlow::Continue(told) = self.tell(input, &mut roster, focus.as_mut())? else {
                break;
            };
            if told.is_some_and(|event| !send(event)) {
                break;
            }
        }
        Ok(())
    }

    /// The stream ends with the agent's program, at once when it ended already or when
    /// herdr-core no longer knows its terminal.
    fn forward_agent_status(
        &self,
        pane_id: &PaneId,
        canceller: &EventCanceller,
        send: &mut dyn FnMut(AgentStatus) -> bool,
    ) -> Result<()> {
        let Some((snapshot, mut events)) =
            self.subscribe(&json!({"terminal": pane_id.0}), canceller)?
        else {
            return Ok(());
        };
        let Some(terminal) = snapshot.iter().find(|terminal| terminal.is_live()) else {
            // Its program ended already: herdr-core would tell nothing more until it removes
            // the terminal.
            return Ok(());
        };
        if let Some(status) = &terminal.status
            && !send(status.agent_status())
        {
            return Ok(());
        }
        while let Some(event) = events.next_event()? {
            if event.ends_program() {
                break;
            }
            if event.kind == "status" && !send(status_of(&event.data::<StatusChange>()?)) {
                break;
            }
        }
        Ok(())
    }

    /// Sets the terminal's session label, as the agent's hook does under herdr-core.
    fn report_agent_session(&self, pane_id: &PaneId, report: &SessionReport<'_>) -> Result<()> {
        self.report_session(pane_id, report.agent, report.session)
    }
}

#[cfg(test)]
#[path = "client.tests.rs"]
mod tests;
