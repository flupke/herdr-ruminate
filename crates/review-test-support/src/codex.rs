use std::ffi::OsStr;
use std::fs;
use std::io::{BufRead, BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Mutex, PoisonError};
use std::thread;

use serde_json::{Value, json};

use herdr_client::protocol::{AgentStatus, HerdrEvent, PaneId};

use crate::{GUARD, PrivateHerdrServer};

pub fn prompt(herdr: &PrivateHerdrServer, pane: &PaneId, text: &str) {
    herdr.run_cli(&["agent", "prompt", &pane.0, text]);
}

const WHERE_TO_RUN: &str = "Where should the new conversation run?";

const EMPTY_INPUT_BOX: &str = "Ask Codex to do anything";

// Codex asks where the session runs, and would take the next prompt's Enter for the answer.
pub fn new_session(herdr: &PrivateHerdrServer, pane: &PaneId) {
    prompt(herdr, pane, "/new");
    wait_output(herdr, pane, WHERE_TO_RUN);
    // Codex's first choice, selected already: "Use current Git worktree".
    herdr.run_cli(&["pane", "send-keys", &pane.0, "enter"]);
    wait_output(herdr, pane, EMPTY_INPUT_BOX);
}

pub fn wait_output(herdr: &PrivateHerdrServer, pane: &PaneId, text: &str) {
    let guard = GUARD.as_millis().to_string();
    herdr.run_cli(&[
        "pane",
        "wait-output",
        &pane.0,
        "--match",
        text,
        "--source",
        "visible",
        "--timeout",
        &guard,
    ]);
}

pub struct CodexHome {
    directory: tempfile::TempDir,
    workspace: PathBuf,
    codex: PathBuf,
}

impl Default for CodexHome {
    fn default() -> Self {
        Self::new()
    }
}

impl CodexHome {
    pub fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let workspace = directory.path().join("workspace");
        fs::create_dir(&workspace).unwrap();
        assert!(
            Command::new("git")
                .args(["init", "--quiet"])
                .arg(&workspace)
                .status()
                .unwrap()
                .success()
        );
        Self::working_in(directory, workspace)
    }

    pub fn in_repository(workspace: &Path) -> Self {
        Self::working_in(tempfile::tempdir().unwrap(), workspace.to_owned())
    }

    fn working_in(directory: tempfile::TempDir, workspace: PathBuf) -> Self {
        for name in ["codex", "home"] {
            fs::create_dir(directory.path().join(name)).unwrap();
        }
        Self {
            directory,
            workspace,
            codex: native_codex(),
        }
    }

    fn root(&self) -> &Path {
        self.directory.path()
    }

    pub fn workspace(&self) -> PathBuf {
        self.workspace.clone()
    }

    pub fn home(&self) -> PathBuf {
        self.root().join("codex")
    }

    pub fn environment(&self) -> [(&'static str, PathBuf); 2] {
        [
            ("CODEX_HOME", self.home()),
            ("HOME", self.root().join("home")),
        ]
    }

    pub fn configure(&self, provider: u16, mcp: &str) {
        let workspace = serde_json::to_string(&self.workspace()).unwrap();
        fs::write(
            self.home().join("config.toml"),
            format!(
                // Codex's TUI announces a model migration, and drops what is typed meanwhile.
                r#"model = "gpt-6.1-sol"
model_provider = "fixture"
approval_policy = "never"
sandbox_mode = "read-only"
check_for_update_on_startup = false

[model_providers.fixture]
name = "Private test provider"
base_url = "http://127.0.0.1:{provider}/v1"
wire_api = "responses"
supports_websockets = false
request_max_retries = 0
stream_max_retries = 0

[projects.{workspace}]
trust_level = "trusted"

[tui]
screen_reader_detection_done = true

{mcp}
[mcp_servers.herdr_reviewer.tools.list_threads]
approval_mode = "approve"
"#
            ),
        )
        .unwrap();
    }

    pub fn start_tui(&self, herdr: &PrivateHerdrServer, arguments: &[&str]) -> PaneId {
        let workspace = herdr.run_cli_json(&[
            "workspace",
            "create",
            "--cwd",
            &self.workspace().to_string_lossy(),
            "--label",
            "codex",
            "--no-focus",
        ]);
        let pane = PaneId(
            workspace["result"]["root_pane"]["pane_id"]
                .as_str()
                .unwrap()
                .to_owned(),
        );
        self.run_tui(herdr, &pane, arguments);
        pane
    }

    pub fn run_tui(&self, herdr: &PrivateHerdrServer, pane: &PaneId, arguments: &[&str]) {
        let events = herdr.events();
        let command = self
            .environment()
            .iter()
            .map(|(name, value)| format!("{name}={}", quoted(&value.to_string_lossy())))
            .chain(std::iter::once(quoted(&self.codex.to_string_lossy())))
            .chain(arguments.iter().map(|argument| quoted(argument)))
            .collect::<Vec<_>>()
            .join(" ");
        herdr.run_cli(&["pane", "run", &pane.0, &format!("env {command}")]);
        events.wait_for("the Codex agent", |event| {
            matches!(event, HerdrEvent::AgentDetected { pane_id, agent: Some(agent), released: false, .. }
                if pane_id == pane && agent == "codex")
        });
        herdr.agent_statuses(pane).wait_for(AgentStatus::Idle);
    }

    pub fn install_herdr_integration(&self, herdr: &PrivateHerdrServer) {
        let output = Command::new(herdr.binary())
            .args(["integration", "install", "codex"])
            .envs(self.environment())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "herdr integration install codex:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Codex runs no hook it does not trust.
    pub fn trust_hooks(&self) {
        let mut server = StdioServer::start(self);
        let listed = server.request("hooks/list", &json!({}));
        let state: serde_json::Map<String, Value> = listed["data"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|directory| directory["hooks"].as_array().into_iter().flatten())
            .map(|hook| {
                (
                    hook["key"].as_str().unwrap().to_owned(),
                    json!({"trusted_hash": hook["currentHash"]}),
                )
            })
            .collect();
        server.request(
            "config/batchWrite",
            &json!({"edits": [{"keyPath": "hooks.state", "mergeStrategy": "upsert", "value": state}]}),
        );
    }
}

// Codex's app server takes one JSON-RPC message per line.
struct StdioServer {
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    next: u64,
    _lifetime: Lifetime,
}

impl StdioServer {
    fn start(home: &CodexHome) -> Self {
        let mut child = Command::new(&home.codex)
            .arg("app-server")
            .envs(home.environment())
            .current_dir(home.workspace())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut server = Self {
            input: child.stdin.take().unwrap(),
            output: BufReader::new(child.stdout.take().unwrap()),
            next: 0,
            _lifetime: Lifetime::new(child),
        };
        server.request(
            "initialize",
            &json!({"clientInfo": {"name": "review-test-support", "version": "0"}}),
        );
        writeln!(server.input, "{}", json!({"method": "initialized"})).unwrap();
        server
    }

    fn request(&mut self, method: &str, params: &Value) -> Value {
        self.next += 1;
        let id = self.next;
        writeln!(
            self.input,
            "{}",
            json!({"id": id, "method": method, "params": params})
        )
        .unwrap();
        loop {
            let mut line = String::new();
            assert_ne!(self.output.read_line(&mut line).unwrap(), 0, "{method}");
            let message: Value = serde_json::from_str(&line).unwrap();
            if message["id"] == id && message.get("method").is_none() {
                assert!(message.get("error").is_none(), "{method}: {message}");
                return message["result"].clone();
            }
        }
    }
}

fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

// codex's npm package runs the native binary under a Node wrapper.
fn native_codex() -> PathBuf {
    let codex = std::env::var_os("PATH")
        .and_then(|path| {
            std::env::split_paths(&path)
                .map(|directory| directory.join("codex"))
                .find(|candidate| candidate.is_file())
        })
        .expect("Codex is not installed")
        .canonicalize()
        .unwrap();
    let package = codex.parent().and_then(Path::parent).unwrap();
    let native = fs::read_dir(package.join("node_modules/@openai"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("codex-linux-")
        })
        .flat_map(|entry| {
            fs::read_dir(entry.path().join("vendor"))
                .into_iter()
                .flatten()
        })
        .flatten()
        .map(|target| target.path().join("bin/codex"))
        .find(|binary| binary.is_file());
    native.unwrap_or(codex)
}

pub struct Provider {
    port: u16,
    prompts: Mutex<Receiver<ExplorePrompt>>,
    outputs: Mutex<Receiver<ToolOutput>>,
    replies: Mutex<ChildStdin>,
    _child: Lifetime,
}

#[derive(Clone, Debug)]
pub struct ExplorePrompt {
    pub access: String,
    pub prompt: String,
}

#[derive(Clone, Debug)]
pub struct ToolOutput {
    pub prompt: String,
    pub output: String,
}

impl Provider {
    pub fn start(codex: &CodexHome, mcp: &str) -> Self {
        let mut child = Command::new("python3")
            .arg("-B")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/codex_provider.py"
            ))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(fs::File::create(codex.root().join("provider.log")).unwrap())
            .spawn()
            .unwrap();
        let mut output = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        let port = line.trim().parse().expect("the provider printed no port");
        let (prompted, prompts) = mpsc::channel();
        let (answered, outputs) = mpsc::channel();
        thread::spawn(move || {
            for line in output.lines().map_while(Result::ok) {
                let event: Value = serde_json::from_str(&line).unwrap();
                let text = |name: &str| event[name].as_str().unwrap_or_default().to_owned();
                if event.get("output").is_some() {
                    let _ = answered.send(ToolOutput {
                        prompt: text("prompt"),
                        output: text("output"),
                    });
                } else {
                    let _ = prompted.send(ExplorePrompt {
                        access: text("access"),
                        prompt: text("prompt"),
                    });
                }
            }
        });
        codex.configure(port, mcp);
        let replies = Mutex::new(child.stdin.take().unwrap());
        Self {
            port,
            prompts: Mutex::new(prompts),
            outputs: Mutex::new(outputs),
            replies,
            _child: Lifetime::new(child),
        }
    }

    pub fn next_prompt(&self) -> ExplorePrompt {
        self.prompts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .recv()
            .expect("the provider stopped")
    }

    pub fn next_output(&self, prompt: &str) -> String {
        let outputs = self.outputs.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            let output = outputs.recv().expect("the provider stopped");
            if output.prompt == prompt {
                return output.output;
            }
        }
    }

    pub fn reply(&self, access: &str, tool: &str, arguments: &Value) {
        let mut replies = self.replies.lock().unwrap_or_else(PoisonError::into_inner);
        writeln!(
            replies,
            "{}",
            json!({"access": access, "tool": tool, "arguments": arguments})
        )
        .unwrap();
    }
}

// Codex's managed daemon updates itself; an unmanaged app server never does.
pub struct AppServer {
    pid: String,
    lifetime: Lifetime,
}

const LISTENING: &str = "app-server control socket listening";

impl AppServer {
    pub fn start(codex: &CodexHome, provider: &Provider, environment: &[(&str, &OsStr)]) -> Self {
        assert_ne!(provider.port, 0);
        let mut command = Command::new("sh");
        command
            .args([
                "-c",
                r#"exec 3<&0; "$@" </dev/null >/dev/null & server=$!; echo "$server"
                   { cat <&3 >/dev/null; kill -TERM "$server" 2>/dev/null; } & wait "$server""#,
                "sh",
            ])
            .arg(&codex.codex)
            .args(["app-server", "--listen", "unix://"])
            .current_dir(codex.workspace());
        for (name, _) in std::env::vars_os() {
            if name.as_encoded_bytes().starts_with(b"HERDR_") {
                command.env_remove(name);
            }
        }
        let mut child = command
            .envs(codex.environment())
            .envs(environment.iter().copied())
            .env("RUST_LOG", "codex_app_server_transport=info")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut pid = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut pid)
            .unwrap();
        let (listening, ready) = mpsc::channel();
        let stderr = child.stderr.take().unwrap();
        let mut log = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(codex.root().join("app-server.log"))
            .unwrap();
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if line.contains(LISTENING) {
                    let _ = listening.send(());
                }
                let _ = writeln!(log, "{line}");
            }
        });
        ready
            .recv_timeout(GUARD)
            .expect("the Codex app server did not listen");
        Self {
            pid: pid.trim().to_owned(),
            lifetime: Lifetime::new(child),
        }
    }

    pub fn stop(mut self) {
        assert!(
            Command::new("kill")
                .args(["-TERM", &self.pid])
                .status()
                .unwrap()
                .success()
        );
        let _ = self.lifetime.child.wait();
    }
}

struct Lifetime {
    child: Child,
    stdin: Option<ChildStdin>,
}

impl Lifetime {
    fn new(mut child: Child) -> Self {
        let stdin = child.stdin.take();
        Self { child, stdin }
    }
}

impl Drop for Lifetime {
    fn drop(&mut self) {
        drop(self.stdin.take());
        let _ = self.child.wait();
    }
}
