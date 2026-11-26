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
    Agent, AgentHost, AgentPort, AgentStatus, HerdrEvent, PaneId, PaneProcessInfo, PromptWait,
    SessionReport, SessionSnapshot, TabId, WorkspaceId,
};
use herdr_client::{Error, Result};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::events::{Event, Roster, StatusChange, status_of};
use crate::focus::Focus;
use crate::terminal::{SESSION_LABEL, Terminal, place_of, session_label};

const SOCKET_TIMEOUT: Duration = Duration::from_secs(5);
// herdr-core's `terminal.send` waits until nobody typed in the terminal for about 3 s, however
// long that takes, and herdr-core 0.1.20 and later cancel a send whose client gave up on it.
const SEND_TIMEOUT: Duration = Duration::from_secs(10 * 60);
static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub struct HcClient {
    socket_path: PathBuf,
    focus_home: Option<AppHome>,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    message: String,
    #[serde(default)]
    data: Option<ErrorData>,
}

#[derive(Debug, Deserialize)]
struct ErrorData {
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

#[derive(Debug, Deserialize)]
struct Sent {
    #[serde(default)]
    started: Option<bool>,
    #[serde(default)]
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Listed {
    terminals: Vec<Terminal>,
}

#[derive(Debug, Deserialize)]
struct Document {
    #[serde(default)]
    value: Option<Layout>,
}

#[derive(Debug, Deserialize)]
struct Snapshot {
    terminals: Vec<Terminal>,
}

struct Subscription {
    reader: BufReader<UnixStream>,
    tie: Tie,
    socket_path: PathBuf,
}

enum Input {
    Event(Result<Option<Event>>),
    ViewWritten,
}

struct Reader(UnixStream);

impl Drop for Reader {
    fn drop(&mut self) {
        let _ = self.0.shutdown(Shutdown::Both);
    }
}

fn watch_view(home: &AppHome, inputs: mpsc::Sender<Input>) -> Option<ViewWatch> {
    home.watch_view(move || {
        let _ = inputs.send(Input::ViewWritten);
    })
    .ok()
}

impl HcClient {
    pub fn new(socket_path: PathBuf) -> Self {
        Self {
            socket_path,
            focus_home: None,
        }
    }

    #[must_use]
    pub fn following_focus(self, home: AppHome) -> Self {
        Self {
            focus_home: Some(home),
            ..self
        }
    }

    pub fn from_env() -> Result<Self> {
        std::env::var_os("HC_TERMINAL_SOCKET")
            .filter(|path| !path.is_empty())
            .map(|path| Self::new(path.into()))
            .ok_or(Error::Environment {
                name: "HC_TERMINAL_SOCKET",
            })
    }

    pub fn labels(&self, terminal: &PaneId) -> Result<BTreeMap<String, String>> {
        let terminal = self.terminal(terminal)?.ok_or_else(|| Error::Herdr {
            operation: "terminal.get",
            message: format!("no terminal {:?}", terminal.0),
        })?;
        Ok(terminal.labels)
    }

    pub fn place(&self, terminal: &PaneId) -> Result<Option<(WorkspaceId, TabId)>> {
        Ok(place_of(&self.labels(terminal)?))
    }

    pub fn set_labels(&self, terminal: &PaneId, labels: &BTreeMap<String, String>) -> Result<()> {
        self.call::<Value>(
            "terminal.label",
            &json!({"terminal": terminal.0, "labels": labels}),
        )
        .map(|_| ())
    }

    pub fn report_session(&self, terminal: &PaneId, agent: &str, session: &str) -> Result<()> {
        self.set_labels(
            terminal,
            &BTreeMap::from([(SESSION_LABEL.to_owned(), session_label(agent, session))]),
        )
    }

    // herdr-core removes a label set to an empty value.
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
                    self.layout()
                        .ok()
                        .and_then(|layout| focus.layout_changed(layout))
                        .map(HerdrEvent::PaneFocused),
                ))
            }
            _ => roster.observe(&event).map(ControlFlow::Continue),
        }
    }

    fn layout(&self) -> Result<Layout> {
        let document: Document = self.call("doc.get", &json!({"name": LAYOUT_DOCUMENT}))?;
        Ok(document.value.unwrap_or_default())
    }

    fn layout_or_none(&self) -> Layout {
        self.layout().unwrap_or_default()
    }

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

    fn send(
        &self,
        pane_id: &PaneId,
        text: &str,
        waits: &mut dyn FnMut(Option<PromptWait>),
        withdrawal: &EventCanceller,
    ) -> Result<Sent> {
        self.send_on_empty_line(pane_id, text, waits, withdrawal)
    }

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
        self.request_on(self.connect(Some(timeout))?, method, params)
    }

    fn request_on(
        &self,
        mut socket: UnixStream,
        method: &'static str,
        params: &Value,
    ) -> Result<std::result::Result<Value, ApiError>> {
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

// herdr-core refuses these before it types: it checks `blocked` again as it types, its
// `unavailable` says nothing was typed, and it checks the others on the request or the terminal
// first. Any other refusal may come after it typed.
fn untyped(error: &ApiError) -> Option<String> {
    let kind = error.data.as_ref()?.kind.as_deref()?;
    Some(match kind {
        "blocked" => "it waits on its user, as for a permission".to_owned(),
        "finished" => "its program ended".to_owned(),
        "not_found" => "herdr-core no longer knows its terminal".to_owned(),
        "unavailable" | "machine_unknown" | "status_mismatch" | "invalid_params" | "config" => {
            error.message.clone()
        }
        _ => return None,
    })
}

fn refusal(method: &'static str, error: ApiError) -> Error {
    Error::Herdr {
        operation: method,
        message: error.message,
    }
}

impl AgentPort for HcClient {
    // herdr-core keeps no focus: herdr-app keeps it, on each of its screens.
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

    fn prompt_agent(&self, pane_id: &PaneId, text: &str) -> Result<()> {
        self.prompt_agent_telling_waits(pane_id, text, &mut |_| {}, &EventCanceller::default())
    }

    fn submit_prompt(&self, pane_id: &PaneId, text: &str) -> Result<()> {
        self.submit_prompt_telling_waits(pane_id, text, &mut |_| {}, &EventCanceller::default())
    }

    // herdr-core's `started` tells whether the agent went working within a few seconds; Herdr also
    // counts an agent that blocked on its user at once as started.
    fn prompt_agent_telling_waits(
        &self,
        pane_id: &PaneId,
        text: &str,
        waits: &mut dyn FnMut(Option<PromptWait>),
        withdrawal: &EventCanceller,
    ) -> Result<()> {
        let sent = self.send(pane_id, text, waits, withdrawal)?;
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

    fn submit_prompt_telling_waits(
        &self,
        pane_id: &PaneId,
        text: &str,
        waits: &mut dyn FnMut(Option<PromptWait>),
        withdrawal: &EventCanceller,
    ) -> Result<()> {
        self.send(pane_id, text, waits, withdrawal).map(|_| ())
    }

    // herdr-core sends an event for each change of a terminal's labels.
    fn announces_sessions(&self) -> bool {
        true
    }

    // herdr-core's `terminal.send` types only once nobody typed for about 3 s, and holds the keys
    // pressed while it types.
    fn types_prompts_whole(&self) -> bool {
        true
    }
}

impl AgentHost for HcClient {
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
        for agent in roster.agents().collect::<Vec<_>>() {
            if !send(HerdrEvent::AgentSessionChanged(agent)) {
                return Ok(());
            }
        }
        let (inputs, received) = mpsc::channel();
        let _reader = events.forward_to(inputs.clone())?;
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
            // herdr-core sends nothing more on a finished terminal until it removes it.
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

    fn report_agent_session(&self, pane_id: &PaneId, report: &SessionReport<'_>) -> Result<()> {
        self.report_session(pane_id, report.agent, report.session)
    }
}

#[path = "line_wait.rs"]
mod line_wait;
#[path = "screen_watch.rs"]
mod screen_watch;

#[cfg(test)]
#[path = "client.tests.rs"]
mod tests;
