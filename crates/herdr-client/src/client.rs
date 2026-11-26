use std::env;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::Shutdown;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::protocol::{
    Agent, AgentHost, AgentPort, AgentStatus, EntrypointId, HerdrEvent, HerdrReader, HerdrWriter,
    OpenPluginPane, PaneId, PaneProcessInfo, PaneSize, PluginPane, SessionReport, SessionSnapshot,
    TabId, WorkspaceId, method,
};
use crate::{Error, Result};

const RESPONSE_LIMIT: u64 = 16 * 1024 * 1024;
const SOCKET_TIMEOUT: Duration = Duration::from_secs(5);
// Herdr answers `agent_prompt_stalled` once an agent shows no activity for 5 seconds.
const PROMPT_START_TIMEOUT: Duration = Duration::from_secs(10);
static EVENT_REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const HERDR_SOCKET: &str = "HERDR_SOCKET_PATH";

#[derive(Clone, Debug)]
pub struct HerdrClient {
    socket_path: PathBuf,
    plugin_id: String,
    state_dir: PathBuf,
    prompt_start_timeout: Duration,
}

#[derive(Clone, Debug, Default)]
pub struct EventCanceller(Arc<Mutex<Cancellation>>);

#[derive(Debug, Default)]
struct Cancellation {
    cancelled: bool,
    sockets: Vec<(u64, UnixStream)>,
    next_tie: u64,
}

impl EventCanceller {
    pub fn cancel(&self) {
        let mut cancellation = self.lock();
        cancellation.cancelled = true;
        for (_, socket) in cancellation.sockets.drain(..) {
            let _ = socket.shutdown(Shutdown::Both);
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.lock().cancelled
    }

    pub fn tie(&self, socket: &UnixStream) -> std::io::Result<Option<Tie>> {
        let mut cancellation = self.lock();
        if cancellation.cancelled {
            return Ok(None);
        }
        let id = cancellation.next_tie;
        cancellation.next_tie += 1;
        cancellation.sockets.push((id, socket.try_clone()?));
        Ok(Some(Tie {
            canceller: self.clone(),
            id,
        }))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Cancellation> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[derive(Debug)]
pub struct Tie {
    canceller: EventCanceller,
    id: u64,
}

impl Tie {
    pub fn is_cancelled(&self) -> bool {
        self.canceller.is_cancelled()
    }
}

impl Drop for Tie {
    fn drop(&mut self) {
        self.canceller
            .lock()
            .sockets
            .retain(|(id, _)| *id != self.id);
    }
}

#[derive(Debug)]
pub struct HerdrEvents(HerdrEventStream);

impl HerdrEvents {
    pub fn forward(mut self, send: impl FnMut(HerdrEvent) -> bool) -> Result<()> {
        self.0.forward_while(send)
    }
}

#[derive(Debug)]
pub struct AgentStatuses {
    stream: HerdrEventStream,
    client: HerdrClient,
    pane_id: PaneId,
}

impl AgentStatuses {
    pub fn forward(mut self, mut send: impl FnMut(AgentStatus) -> bool) -> Result<()> {
        if let Some(agent) = self.client.get_agent(&self.pane_id)?
            && !send(agent.agent_status)
        {
            return Ok(());
        }
        while let Some(status) = self.stream.next_status()? {
            if !send(status) {
                break;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct Response {
    id: String,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<ApiError>,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    code: String,
    message: String,
}

#[derive(Debug, Deserialize)]
pub struct EventEnvelope {
    pub event: String,
    pub data: Value,
}

#[derive(Debug, Deserialize)]
struct FocusEvent {
    pane_id: PaneId,
}

#[derive(Debug, Deserialize)]
struct AgentStatusEvent {
    agent_status: AgentStatus,
}

#[derive(Debug, Deserialize)]
struct AgentDetectedEvent {
    pane_id: PaneId,
    workspace_id: WorkspaceId,
    #[serde(default)]
    agent: Option<String>,
    #[serde(default)]
    released: bool,
    #[serde(default)]
    final_status: Option<AgentStatus>,
}

#[derive(Debug, Deserialize)]
struct PaneWire {
    #[serde(rename = "pane_id")]
    pane: PaneId,
    #[serde(rename = "tab_id")]
    tab: TabId,
    #[serde(rename = "workspace_id")]
    workspace: WorkspaceId,
}

#[derive(Debug, Deserialize)]
struct PluginPaneWire {
    entrypoint: EntrypointId,
    pane: PaneWire,
}

#[derive(Debug, Deserialize)]
struct PaneLayoutWire {
    panes: Vec<LayoutPaneWire>,
}

#[derive(Debug, Deserialize)]
struct LayoutPaneWire {
    pane_id: PaneId,
    rect: PaneSize,
}

#[derive(Debug, Deserialize)]
struct PaneReadWire {
    text: String,
}

#[derive(Debug)]
pub struct HerdrEventStream {
    reader: BufReader<UnixStream>,
    tie: Tie,
}

impl HerdrEventStream {
    pub fn next_event(&mut self) -> Result<Option<EventEnvelope>> {
        self.read_envelope()
    }

    fn forward_while(&mut self, mut send: impl FnMut(HerdrEvent) -> bool) -> Result<()> {
        while let Some(envelope) = self.read_envelope()? {
            if let Some(event) = parse_stream_event(envelope)?
                && !send(event)
            {
                break;
            }
        }
        Ok(())
    }

    fn next_status(&mut self) -> Result<Option<AgentStatus>> {
        while let Some(envelope) = self.read_envelope()? {
            // Herdr names this event with a dot, as its subscription does, and its API schema with
            // an underscore.
            if matches!(
                envelope.event.as_str(),
                "pane.agent_status_changed" | "pane_agent_status_changed"
            ) {
                let event: AgentStatusEvent =
                    parse_event(envelope.data, "read Herdr agent status event")?;
                return Ok(Some(event.agent_status));
            }
        }
        Ok(None)
    }

    fn read_envelope(&mut self) -> Result<Option<EventEnvelope>> {
        let line = match read_line(
            &mut self.reader,
            "read Herdr event",
            Path::new(HERDR_SOCKET),
        ) {
            Ok(line) => line,
            Err(_) if self.tie.is_cancelled() => return Ok(None),
            Err(error) => return Err(error),
        };
        if line.is_empty() {
            if self.tie.is_cancelled() {
                return Ok(None);
            }
            return Err(Error::Protocol {
                operation: "read Herdr event".to_owned(),
                detail: "Herdr closed the event stream",
            });
        }
        serde_json::from_slice(&line)
            .map(Some)
            .map_err(|source| Error::Json {
                operation: "read Herdr event",
                source,
            })
    }
}

fn parse_stream_event(event: EventEnvelope) -> Result<Option<HerdrEvent>> {
    Ok(match event.event.as_str() {
        "pane_focused" => {
            let value: FocusEvent = parse_event(event.data, "read Herdr focus event")?;
            Some(HerdrEvent::PaneFocused(value.pane_id))
        }
        "pane_agent_detected" => {
            let value: AgentDetectedEvent =
                parse_event(event.data, "read Herdr agent detection event")?;
            Some(HerdrEvent::AgentDetected {
                pane_id: value.pane_id,
                workspace_id: value.workspace_id,
                agent: value.agent,
                released: value.released,
                final_status: value.final_status,
            })
        }
        _ => None,
    })
}

impl HerdrClient {
    fn agent_screen(&self, pane_id: &PaneId) -> Result<String> {
        let result = self.request(
            method::AGENT_READ,
            &json!({
                "target": pane_id.0,
                "source": "visible",
                "format": "text",
                "strip_ansi": true,
            }),
        )?;
        let read: PaneReadWire = Self::parse(&result, "read", method::AGENT_READ)?;
        Ok(read.text)
    }

    pub fn new(socket_path: PathBuf, plugin_id: String, state_dir: PathBuf) -> Self {
        Self {
            socket_path,
            plugin_id,
            state_dir,
            prompt_start_timeout: PROMPT_START_TIMEOUT,
        }
    }

    #[must_use]
    pub fn with_prompt_start_timeout(mut self, timeout: Duration) -> Self {
        self.prompt_start_timeout = timeout;
        self
    }

    pub fn socket_path(&self) -> &std::path::Path {
        &self.socket_path
    }

    pub fn from_env() -> Result<Self> {
        Ok(Self::new(
            required_path("HERDR_SOCKET_PATH")?,
            required_string("HERDR_PLUGIN_ID")?,
            required_path("HERDR_PLUGIN_STATE_DIR")?,
        ))
    }

    pub fn subscribe_events(&self, canceller: &EventCanceller) -> Result<HerdrEvents> {
        self.subscribe(&event_subscriptions(), canceller)
            .map(HerdrEvents)
    }

    pub fn subscribe_agent_status(
        &self,
        pane_id: &PaneId,
        canceller: &EventCanceller,
    ) -> Result<AgentStatuses> {
        let stream = self.subscribe(
            &[json!({"type": "pane.agent_status_changed", "pane_id": pane_id.0})],
            canceller,
        )?;
        Ok(AgentStatuses {
            stream,
            client: self.clone(),
            pane_id: pane_id.clone(),
        })
    }

    pub fn subscribe(
        &self,
        subscriptions: &[Value],
        canceller: &EventCanceller,
    ) -> Result<HerdrEventStream> {
        let mut socket = self.connect(None)?;
        let tie = canceller
            .tie(&socket)
            .map_err(|source| Error::Io {
                operation: "configure Herdr event cancellation",
                path: self.socket_path.clone(),
                source,
            })?
            .ok_or(Error::Cancelled)?;
        let request_id = format!(
            "progressive-reviewer-events-{}-{}",
            std::process::id(),
            EVENT_REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        let request = json!({
            "id": &request_id,
            "method": "events.subscribe",
            "params": {"subscriptions": subscriptions}
        });
        let or_cancellation = |error| {
            if canceller.is_cancelled() {
                Error::Cancelled
            } else {
                error
            }
        };
        write_json_line(
            &mut socket,
            &request,
            "subscribe to Herdr events",
            &self.socket_path,
        )
        .map_err(or_cancellation)?;
        let mut reader = BufReader::new(socket);
        let line = read_line(&mut reader, "subscribe to Herdr events", &self.socket_path)
            .map_err(or_cancellation)?;
        if line.is_empty() && canceller.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let response: Response = serde_json::from_slice(&line).map_err(|source| Error::Json {
            operation: "subscribe to Herdr events",
            source,
        })?;
        if let Err(error) = validate_response(response, &request_id, "subscribe to Herdr events")? {
            return Err(Error::Herdr {
                operation: "subscribe to Herdr events",
                message: error.message,
            });
        }
        Ok(HerdrEventStream { reader, tie })
    }

    pub fn request(&self, operation: &'static str, params: &Value) -> Result<Value> {
        match self.response(operation, params, SOCKET_TIMEOUT)? {
            Ok(result) => Ok(result),
            Err(error) => Err(Error::Herdr {
                operation,
                message: error.message,
            }),
        }
    }

    fn response(
        &self,
        operation: &'static str,
        params: &Value,
        timeout: Duration,
    ) -> Result<std::result::Result<Value, ApiError>> {
        let mut socket = self.connect(Some(timeout))?;
        let request = json!({
            "id": "progressive-reviewer",
            "method": operation,
            "params": params,
        });
        write_json_line(&mut socket, &request, operation, &self.socket_path)?;
        let response: Response = serde_json::from_slice(&read_line(
            &mut BufReader::new(socket),
            operation,
            &self.socket_path,
        )?)
        .map_err(|source| Error::Json { operation, source })?;
        validate_response(response, "progressive-reviewer", operation)
    }

    fn connect(&self, timeout: Option<Duration>) -> Result<UnixStream> {
        let socket = UnixStream::connect(&self.socket_path).map_err(|source| Error::Io {
            operation: "connect to Herdr",
            path: self.socket_path.clone(),
            source,
        })?;
        socket
            .set_read_timeout(timeout)
            .and_then(|()| socket.set_write_timeout(timeout))
            .map_err(|source| Error::Io {
                operation: "configure Herdr socket",
                path: self.socket_path.clone(),
                source,
            })?;
        Ok(socket)
    }

    fn parse<T: for<'de> Deserialize<'de>>(
        value: &Value,
        field: &'static str,
        operation: &'static str,
    ) -> Result<T> {
        serde_json::from_value(value.get(field).cloned().ok_or(Error::Protocol {
            operation: operation.to_owned(),
            detail: "Herdr response did not contain the required field",
        })?)
        .map_err(|source| Error::Json { operation, source })
    }

    fn pane_path(&self, tab_id: &TabId) -> PathBuf {
        let mut key = String::with_capacity(tab_id.0.len() * 2);
        for byte in tab_id.0.bytes() {
            use std::fmt::Write as _;
            write!(key, "{byte:02x}").expect("writing to a string cannot fail");
        }
        self.state_dir.join("panes").join(format!("{key}.json"))
    }

    fn save_pane(&self, pane: &PluginPane) -> Result<()> {
        let target = self.pane_path(&pane.tab_id);
        let directory = target.parent().expect("a pane record has a parent");
        fs::create_dir_all(directory).map_err(|source| Error::Io {
            operation: "create Herdr pane state",
            path: directory.to_owned(),
            source,
        })?;
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).map_err(|source| {
            Error::Io {
                operation: "secure Herdr pane state",
                path: directory.to_owned(),
                source,
            }
        })?;
        let temporary = target.with_extension(format!("tmp-{}", std::process::id()));
        let bytes = serde_json::to_vec(pane).map_err(|source| Error::Json {
            operation: "store Herdr pane state",
            source,
        })?;
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|source| Error::Io {
                operation: "store Herdr pane state",
                path: temporary.clone(),
                source,
            })?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .and_then(|()| fs::rename(&temporary, &target))
            .map_err(|source| Error::Io {
                operation: "store Herdr pane state",
                path: target,
                source,
            })
    }

    fn load_pane(&self, tab_id: &TabId) -> Result<Option<PluginPane>> {
        let target = self.pane_path(tab_id);
        let bytes = match fs::read(&target) {
            Ok(bytes) => bytes,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(Error::Io {
                    operation: "read Herdr pane state",
                    path: target,
                    source,
                });
            }
        };
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|source| Error::Json {
                operation: "read Herdr pane state",
                source,
            })
    }

    fn tab_pane(&self, tab_id: &TabId) -> Result<Option<PluginPane>> {
        if let Some(pane) = self.load_pane(tab_id)? {
            return Ok(Some(pane));
        }
        let Some((path, pane)) = self
            .pane_records()?
            .into_iter()
            .find(|(_, pane)| pane.tab_id == *tab_id)
        else {
            return Ok(None);
        };
        self.save_pane(&pane)?;
        let _ = fs::remove_file(path);
        Ok(Some(pane))
    }

    fn remove_pane(&self, pane_id: &PaneId) -> Result<()> {
        for (path, pane) in self.pane_records()? {
            if pane.pane_id == *pane_id {
                let _ = fs::remove_file(path);
            }
        }
        Ok(())
    }

    fn pane_records(&self) -> Result<Vec<(PathBuf, PluginPane)>> {
        let directory = self.state_dir.join("panes");
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(Error::Io {
                    operation: "scan Herdr pane state",
                    path: directory,
                    source,
                });
            }
        };
        Ok(entries
            .flatten()
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "json")
            })
            .filter_map(|entry| {
                let bytes = fs::read(entry.path()).ok()?;
                let pane = serde_json::from_slice::<PluginPane>(&bytes).ok()?;
                Some((entry.path(), pane))
            })
            .collect())
    }

    fn synchronize_pane_size(&self, pane_id: &PaneId) -> Result<()> {
        // Herdr 0.9 can leave a plugin pane at its parent PTY's size: a zero-distance resize
        // applies the visible size without moving the split or the focus.
        self.request(
            "pane.resize",
            &json!({"pane_id": pane_id.0, "direction": "left", "amount": 0.0}),
        )?;
        Ok(())
    }
}

fn parse_event<T: for<'de> Deserialize<'de>>(data: Value, operation: &'static str) -> Result<T> {
    serde_json::from_value(data).map_err(|source| Error::Json { operation, source })
}

fn event_subscriptions() -> Vec<Value> {
    vec![
        json!({"type": "pane.focused"}),
        json!({"type": "pane.agent_detected"}),
    ]
}

// Herdr's `agent.prompt` fails with `agent_prompt_stalled` after 5 seconds without agent
// activity, and with `timeout` when the request's own wait runs out.
fn is_not_started(code: &str) -> bool {
    matches!(code, "agent_prompt_stalled" | "timeout")
}

fn validate_response(
    response: Response,
    expected_id: &str,
    operation: &'static str,
) -> Result<std::result::Result<Value, ApiError>> {
    if response.id != expected_id {
        return Err(Error::Protocol {
            operation: operation.to_owned(),
            detail: "Herdr returned a response with the wrong request ID",
        });
    }
    match (response.result, response.error) {
        (Some(result), None) => Ok(Ok(result)),
        (None, Some(error)) => Ok(Err(error)),
        _ => Err(Error::Protocol {
            operation: operation.to_owned(),
            detail: "Herdr returned neither one result nor one error",
        }),
    }
}

impl AgentPort for HerdrClient {
    fn session_snapshot(&self) -> Result<SessionSnapshot> {
        let result = self.request(method::SESSION_SNAPSHOT, &json!({}))?;
        Self::parse(&result, "snapshot", method::SESSION_SNAPSHOT)
    }

    fn list_agents(&self) -> Result<Vec<Agent>> {
        let result = self.request(method::AGENT_LIST, &json!({}))?;
        Self::parse(&result, "agents", method::AGENT_LIST)
    }

    fn get_agent(&self, pane_id: &PaneId) -> Result<Option<Agent>> {
        match self.response(
            method::AGENT_GET,
            &json!({"target": pane_id.0}),
            SOCKET_TIMEOUT,
        )? {
            Ok(result) => Self::parse(&result, "agent", method::AGENT_GET).map(Some),
            Err(error) if error.code.contains("not_found") => Ok(None),
            Err(error) => Err(Error::Herdr {
                operation: method::AGENT_GET,
                message: error.message,
            }),
        }
    }

    fn pane_process_info(&self, pane_id: &PaneId) -> Result<PaneProcessInfo> {
        let result = self.request("pane.process_info", &json!({"pane_id": pane_id.0}))?;
        Self::parse(&result, "process_info", "pane.process_info")
    }

    fn submit_prompt(&self, pane_id: &PaneId, text: &str) -> Result<()> {
        self.request(
            method::AGENT_PROMPT,
            &json!({"target": pane_id.0, "text": text}),
        )
        .map(|_| ())
    }

    fn prompt_agent(&self, pane_id: &PaneId, text: &str) -> Result<()> {
        let wait_ms = u64::try_from(self.prompt_start_timeout.as_millis()).unwrap_or(u64::MAX);
        let params = json!({
            "target": pane_id.0,
            "text": text,
            "wait": {"until": ["working", "blocked"], "timeout_ms": wait_ms},
        });
        match self.response(
            method::AGENT_PROMPT,
            &params,
            self.prompt_start_timeout + SOCKET_TIMEOUT,
        )? {
            Ok(_) => Ok(()),
            Err(error) if is_not_started(&error.code) => Err(Error::AgentNotStarted {
                message: error.message,
            }),
            Err(error) => Err(Error::Herdr {
                operation: method::AGENT_PROMPT,
                message: error.message,
            }),
        }
    }
}

impl AgentHost for HerdrClient {
    fn forward_events(
        &self,
        canceller: &EventCanceller,
        send: &mut dyn FnMut(HerdrEvent) -> bool,
    ) -> Result<()> {
        self.subscribe_events(canceller)?.forward(send)
    }

    fn forward_agent_status(
        &self,
        pane_id: &PaneId,
        canceller: &EventCanceller,
        send: &mut dyn FnMut(AgentStatus) -> bool,
    ) -> Result<()> {
        self.subscribe_agent_status(pane_id, canceller)?
            .forward(send)
    }

    fn report_agent_session(&self, pane_id: &PaneId, report: &SessionReport<'_>) -> Result<()> {
        self.request(
            "pane.report_agent_session",
            &json!({
                "pane_id": pane_id.0,
                "source": report.source,
                "agent": report.agent,
                "seq": report.seq,
                "agent_session_id": report.session,
                "session_start_source": report.start,
            }),
        )
        .map(|_| ())
    }
}

impl HerdrReader for HerdrClient {
    fn read_agent_screen(&self, pane_id: &PaneId) -> Result<String> {
        self.agent_screen(pane_id)
    }

    fn list_plugin_panes(&self, tab_id: &TabId) -> Result<Vec<PluginPane>> {
        let Some(pane) = self.tab_pane(tab_id)? else {
            return Ok(Vec::new());
        };
        match self.response(
            method::PANE_GET,
            &json!({"pane_id": pane.pane_id.0}),
            SOCKET_TIMEOUT,
        )? {
            Ok(result) => {
                let current: PaneWire = Self::parse(&result, "pane", method::PANE_GET)?;
                if current.tab == *tab_id {
                    Ok(vec![pane])
                } else {
                    let _ = fs::remove_file(self.pane_path(tab_id));
                    if self.load_pane(&current.tab)?.is_none() {
                        self.save_pane(&PluginPane {
                            tab_id: current.tab,
                            ..pane
                        })?;
                    }
                    Ok(Vec::new())
                }
            }
            Err(error) if error.code.contains("not_found") => {
                self.remove_pane(&pane.pane_id)?;
                Ok(Vec::new())
            }
            Err(error) => Err(Error::Herdr {
                operation: method::PANE_GET,
                message: error.message,
            }),
        }
    }

    fn pane_size(&self, pane_id: &PaneId) -> Result<PaneSize> {
        let result = self.request(method::PANE_LAYOUT, &json!({"pane_id": pane_id.0}))?;
        let layout: PaneLayoutWire = Self::parse(&result, "layout", method::PANE_LAYOUT)?;
        layout
            .panes
            .into_iter()
            .find(|pane| pane.pane_id == *pane_id)
            .map(|pane| pane.rect)
            .ok_or_else(|| Error::Protocol {
                operation: method::PANE_LAYOUT.to_owned(),
                detail: "Herdr layout did not list the pane",
            })
    }
}

impl HerdrWriter for HerdrClient {
    fn open_plugin_pane(&self, request: &OpenPluginPane) -> Result<PluginPane> {
        let result = self.request(
            method::PLUGIN_PANE_OPEN,
            &json!({
                "plugin_id": self.plugin_id,
                "entrypoint": request.entrypoint,
                "placement": request.placement,
                "target_pane_id": request.target_pane_id,
                "direction": request.direction,
                "cwd": request.cwd,
                "focus": request.focus,
            }),
        )?;
        let wire: PluginPaneWire = Self::parse(&result, "plugin_pane", method::PLUGIN_PANE_OPEN)?;
        let pane = PluginPane {
            pane_id: wire.pane.pane,
            tab_id: wire.pane.tab,
            workspace_id: wire.pane.workspace,
            entrypoint_id: wire.entrypoint,
        };
        self.save_pane(&pane)?;
        self.synchronize_pane_size(&pane.pane_id)?;
        Ok(pane)
    }

    fn focus_plugin_pane(&self, pane_id: &PaneId) -> Result<()> {
        self.request(method::PLUGIN_PANE_FOCUS, &json!({"pane_id": pane_id.0}))?;
        self.synchronize_pane_size(pane_id)
    }

    fn focus_agent(&self, pane_id: &PaneId) -> Result<()> {
        self.request(method::AGENT_FOCUS, &json!({"target": pane_id.0}))?;
        Ok(())
    }

    fn close_plugin_pane(&self, pane_id: &PaneId) -> Result<()> {
        self.request(method::PLUGIN_PANE_CLOSE, &json!({"pane_id": pane_id.0}))?;
        self.remove_pane(pane_id)
    }
}

fn required_path(name: &'static str) -> Result<PathBuf> {
    env::var_os(name)
        .map(PathBuf::from)
        .ok_or(Error::Environment { name })
}

fn required_string(name: &'static str) -> Result<String> {
    env::var(name).map_err(|_| Error::Environment { name })
}

pub fn write_json_line(
    socket: &mut UnixStream,
    value: &Value,
    operation: &'static str,
    path: &Path,
) -> Result<()> {
    serde_json::to_writer(&mut *socket, value)
        .map_err(|source| Error::Json { operation, source })?;
    socket.write_all(b"\n").map_err(|source| Error::Io {
        operation,
        path: path.to_owned(),
        source,
    })
}

pub fn read_line<R: BufRead>(
    reader: &mut R,
    operation: &'static str,
    path: &Path,
) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    Read::by_ref(reader)
        .take(RESPONSE_LIMIT + 1)
        .read_until(b'\n', &mut bytes)
        .map_err(|source| Error::Io {
            operation,
            path: path.to_owned(),
            source,
        })?;
    if bytes.len() as u64 > RESPONSE_LIMIT {
        return Err(Error::Protocol {
            operation: operation.to_owned(),
            detail: "the host's response exceeded 16 MiB",
        });
    }
    Ok(bytes)
}

#[cfg(test)]
#[path = "client.tests.rs"]
mod tests;
