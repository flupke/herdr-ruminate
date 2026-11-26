#![allow(
    clippy::missing_panics_doc,
    reason = "test fixture methods fail fast by design"
)]

use std::ffi::{OsStr, OsString};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use hc_client::HcClient;
use herdr_client::protocol::PaneId;
use serde_json::{Value, json};

const SESSION: &str = "t";

pub fn report(state: &str) -> String {
    format!(r"printf '\033]7501;app=claude-code:state={state}\007';")
}

pub fn input_line() -> &'static str {
    r"stty -echo; printf '\n──── worker ─\n❯ \n────────────\n';"
}

pub fn line_agent(before: &str) -> String {
    agent_of_line(
        before,
        &format!("{} draw; {}", report("working"), report("idle")),
    )
}

pub fn line_agent_without_turns(before: &str) -> String {
    agent_of_line(before, "")
}

pub fn line_agent_keeping_its_line(before: &str) -> String {
    agent_of_line(before, r#"typed="$(tail -n 1 lines)""#)
}

fn agent_of_line(before: &str, on_enter: &str) -> String {
    format!(
        r#"{before} stty -echo -icanon; typed=''; mode=line
draw() {{ if [ "$mode" = line ]; then printf '\033[H\033[2J──── worker ─\r\n❯ %s\r\n────────────\r\n' "$typed"; else printf '\033[H\033[2JRewind to:\r\n  1. earlier\r\n  2. now\r\n'; fi; }}
draw; {idle}
while IFS= read -rsn1 -d '' key; do
  printf '%s' "$key" >> keys
  if [ "$mode" = dialog ]; then
    if [ "$key" = $'\x1b' ]; then mode=line; draw; else printf '%s' "$key" >> dialog; fi
    continue
  fi
  case "$key" in
    $'\n') date +%s%N >> submitted; printf '%s\n' "$typed" >> lines; typed=''; {on_enter} ;;
    $'\x12') mode=dialog ;;
    $'\x15') typed='' ;;
    $'\x7f') typed="${{typed%?}}" ;;
    *) typed+="$key" ;;
  esac
  draw
done"#,
        idle = report("idle"),
    )
}

pub fn written(folder: &std::path::Path, name: &str) -> String {
    std::fs::read_to_string(folder.join(name)).unwrap_or_default()
}

pub fn wait_until_read(folder: &std::path::Path, key: char) {
    while !written(folder, "keys").contains(key) {
        thread::sleep(Duration::from_millis(10));
    }
}

pub struct PrivateCore {
    // Linux limits a Unix socket's path to 107 bytes.
    runtime: tempfile::TempDir,
    home: tempfile::TempDir,
}

impl PrivateCore {
    pub fn start() -> Self {
        Self::start_with(&[])
    }

    // herdr-core 0.1.20 and later hold a send behind unsent typing for `HC_DRAFT_MS` (60 s).
    pub fn holding_unsent_typing_for(hold: Duration) -> Self {
        Self::start_with(&[("HC_DRAFT_MS", hold.as_millis().to_string())])
    }

    fn start_with(settings: &[(&str, String)]) -> Self {
        let core = Self {
            runtime: tempfile::Builder::new()
                .prefix("hc")
                .tempdir_in("/tmp")
                .unwrap(),
            home: tempfile::tempdir().unwrap(),
        };
        let output = core
            .command(&["server", "start", "--json"])
            .envs(settings.iter().map(|(name, value)| (name, value)))
            .output()
            .unwrap();
        assert!(output.status.success(), "hc server start: {output:?}");
        core
    }

    pub fn runtime(&self) -> &Path {
        self.runtime.path()
    }

    pub fn hc(&self, arguments: &[&str]) -> String {
        let output = self.command(arguments).output().unwrap();
        assert!(output.status.success(), "hc {arguments:?}: {output:?}");
        String::from_utf8(output.stdout).unwrap()
    }

    pub fn command(&self, arguments: &[&str]) -> Command {
        let mut command = Command::new(hc_binary());
        command.args(arguments);
        self.isolate(&mut command);
        command
    }

    // A Herdr or a herdr-core this process runs under sets these, which `hc` would follow.
    fn isolate(&self, command: &mut Command) {
        for name in [
            "HC_TERMINAL_ID",
            "HC_TERMINAL_SOCKET",
            "HC_SCOPE",
            "HERDR_SOCKET_PATH",
            "HERDR_PANE_ID",
        ] {
            command.env_remove(name);
        }
        command.envs(self.environment());
    }

    fn environment(&self) -> Vec<(&'static str, OsString)> {
        let folder = |name: &str| self.home.path().join(name).into_os_string();
        vec![
            ("HC_SESSION", SESSION.into()),
            ("XDG_RUNTIME_DIR", self.runtime.path().into()),
            ("XDG_STATE_HOME", folder("state")),
            ("XDG_CONFIG_HOME", folder("config")),
            ("XDG_DATA_HOME", folder("data")),
            ("XDG_CACHE_HOME", folder("cache")),
            ("HERDR_BUS_DB", folder("bus.db")),
        ]
    }

    pub fn socket(&self) -> PathBuf {
        self.runtime
            .path()
            .join("herdr-core")
            .join(format!("{SESSION}.sock"))
    }

    pub fn client(&self) -> HcClient {
        HcClient::new(self.socket())
    }

    pub fn set_document(&self, name: &str, value: &Value) {
        let mut socket = UnixStream::connect(self.socket()).unwrap();
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "doc.set",
            "params": {"name": name, "value": value, "version": 0},
        });
        writeln!(socket, "{request}").unwrap();
        let mut answer = String::new();
        BufReader::new(socket).read_line(&mut answer).unwrap();
        let answer: Value = serde_json::from_str(&answer).unwrap();
        assert!(answer["error"].is_null(), "doc.set {name}: {answer}");
    }

    pub fn run(&self, folder: &Path, script: &str) -> PaneId {
        self.start_in(folder, &["bash", "-c", script])
    }

    pub fn start_in(&self, folder: &Path, argv: &[&str]) -> PaneId {
        let folder = folder.to_str().unwrap();
        let output = self
            .command(&["run", "--bg", "--json", "--cwd", folder])
            .args([
                "--label",
                "herdr.space=sp1",
                "--label",
                "herdr.tab=tb1",
                "--",
            ])
            .args(argv)
            .output()
            .unwrap();
        assert!(output.status.success(), "hc run {argv:?}: {output:?}");
        let started: Value = serde_json::from_slice(&output.stdout).unwrap();
        PaneId(started["address"].as_str().unwrap().to_owned())
    }

    pub fn screen(&self, terminal: &PaneId) -> String {
        self.command(&["read", &terminal.0]).output().map_or_else(
            |error| format!("(unread: {error})"),
            |read| String::from_utf8_lossy(&read.stdout).into_owned(),
        )
    }

    // herdr-core tells a terminal's foreground process only in its raw events.
    pub fn wait_for_foreground(&self, terminal: &PaneId) {
        let mut socket = UnixStream::connect(self.socket()).unwrap();
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "events.subscribe",
            "params": {"terminal": terminal.0},
        });
        writeln!(socket, "{request}").unwrap();
        for line in BufReader::new(socket).lines() {
            let message: Value = serde_json::from_str(&line.unwrap()).unwrap();
            let named = !message["result"]["terminals"][0]["foreground"].is_null()
                || message["params"]["type"] == "foreground";
            if named {
                return;
            }
        }
        panic!("herdr-core closed the stream before it named the foreground process");
    }

    pub fn person_at(&self, terminal: &PaneId) -> Person {
        // `hc attach` wants a terminal: `script` gives it one, and passes it what it is piped.
        let attach = format!("{} attach {}", quoted(&hc_binary()), terminal.0);
        let mut command = Command::new("script");
        command.args(["-qfec", &attach, "/dev/null"]);
        self.isolate(&mut command);
        let mut window = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let keys = window.stdin.take().unwrap();
        let mut screen = window.stdout.take().unwrap();
        // `hc attach` takes keys only once it drew the terminal, which herdr-core tells nowhere.
        let (drawn, drawing) = mpsc::channel();
        thread::spawn(move || {
            let mut chunk = [0; 4096];
            while screen.read(&mut chunk).is_ok_and(|read| read > 0) {
                let _ = drawn.send(());
            }
        });
        drawing
            .recv()
            .expect("the window closed before it drew the terminal");
        Person {
            window,
            keys: Some(keys),
        }
    }
}

impl Drop for PrivateCore {
    fn drop(&mut self) {
        let _ = self.command(&["server", "stop", "--json"]).output();
    }
}

pub struct Person {
    window: Child,
    keys: Option<ChildStdin>,
}

impl Person {
    pub fn types(&mut self, text: &str, pace: Duration) {
        for key in text.chars() {
            let mut bytes = [0; 4];
            self.press(key.encode_utf8(&mut bytes).as_bytes());
            thread::sleep(pace);
        }
    }

    pub fn presses_enter(&mut self) {
        self.press(b"\r");
    }

    pub fn presses_backspace(&mut self) {
        self.press(b"\x7f");
    }

    pub fn presses_esc(&mut self) {
        self.press(b"\x1b");
    }

    pub fn erases_line(&mut self) {
        self.press(b"\x15");
    }

    fn press(&mut self, key: &[u8]) {
        let keys = self.keys.as_mut().unwrap();
        keys.write_all(key).unwrap();
        keys.flush().unwrap();
    }
}

impl Drop for Person {
    fn drop(&mut self) {
        if let Some(mut keys) = self.keys.take() {
            // Ctrl+\ detaches an `hc attach`.
            if keys.write_all(b"\x1c").and_then(|()| keys.flush()).is_err() {
                let _ = self.window.kill();
            }
        }
        let _ = self.window.wait();
    }
}

fn quoted(path: &OsStr) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', r"'\''"))
}

fn hc_binary() -> OsString {
    std::env::var_os("TEST_HC_BIN_PATH").unwrap_or_else(|| OsString::from("hc"))
}
