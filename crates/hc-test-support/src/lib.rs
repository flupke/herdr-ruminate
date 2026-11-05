//! A herdr-core (`hc`) of a test's own, never the user's: its own `HC_SESSION` and scratch
//! `XDG_*` folders, stopped with the test. The tests that use it are opt-in, since each starts a
//! core of its own: they take the dev shell's pinned `hc` (`TEST_HC_BIN_PATH`), or else `hc` on
//! the `PATH`.

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

/// A herdr-core started for one test, and stopped with it.
pub struct PrivateCore {
    /// Short, under `/tmp`: the socket's path must fit in 107 bytes.
    runtime: tempfile::TempDir,
    home: tempfile::TempDir,
}

impl PrivateCore {
    pub fn start() -> Self {
        let core = Self {
            runtime: tempfile::Builder::new()
                .prefix("hc")
                .tempdir_in("/tmp")
                .unwrap(),
            home: tempfile::tempdir().unwrap(),
        };
        core.hc(&["server", "start", "--json"]);
        core
    }

    /// The runtime folder of the core, the `XDG_RUNTIME_DIR` of its terminals.
    pub fn runtime(&self) -> &Path {
        self.runtime.path()
    }

    /// Runs `hc` with `arguments` on this core, and returns what it printed; fails the test
    /// when it fails.
    pub fn hc(&self, arguments: &[&str]) -> String {
        let output = self.command(arguments).output().unwrap();
        assert!(output.status.success(), "hc {arguments:?}: {output:?}");
        String::from_utf8(output.stdout).unwrap()
    }

    /// `hc` with `arguments` on this core, in this process's environment but for the hosts'
    /// variables, this core's alone. A terminal takes the environment of the core, which
    /// [`Self::start`] started with that of the test, not that of the `hc run` that starts
    /// it: start the program through `env` to change it.
    pub fn command(&self, arguments: &[&str]) -> Command {
        let mut command = Command::new(hc_binary());
        command.args(arguments);
        self.isolate(&mut command);
        command
    }

    /// Gives `command` this core's variables in place of those of the hosts this process runs
    /// under, a Herdr or another herdr-core.
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

    /// The variables that make `hc` talk to this core.
    fn environment(&self) -> Vec<(&'static str, OsString)> {
        let folder = |name: &str| self.home.path().join(name).into_os_string();
        vec![
            ("HC_SESSION", SESSION.into()),
            ("XDG_RUNTIME_DIR", self.runtime.path().into()),
            ("XDG_STATE_HOME", folder("state")),
            ("XDG_CONFIG_HOME", folder("config")),
            ("XDG_DATA_HOME", folder("data")),
            ("XDG_CACHE_HOME", folder("cache")),
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

    /// Runs `script` in a new terminal of the tab `tb1`, in `folder`; returns its address.
    pub fn run(&self, folder: &Path, script: &str) -> PaneId {
        self.start_in(folder, &["bash", "-c", script])
    }

    /// Runs `argv` in a new terminal of the tab `tb1`, in `folder`; returns its address.
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

    /// What `terminal` shows, and its history; read as a test fails, so it never panics.
    pub fn screen(&self, terminal: &PaneId) -> String {
        self.command(&["read", &terminal.0]).output().map_or_else(
            |error| format!("(unread: {error})"),
            |read| String::from_utf8_lossy(&read.stdout).into_owned(),
        )
    }

    /// Waits until herdr-core names the foreground process of `terminal`, which it tells only
    /// in its raw events.
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

    /// A person at a window attached to `terminal`, whose keys herdr-core tells from those a
    /// program sends: `terminal.send` waits until they stop typing.
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
        // The window takes keys once it drew the terminal, which herdr-core tells no other way.
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

/// A person who types in a terminal of a [`PrivateCore`], through a window attached to it.
pub struct Person {
    window: Child,
    /// `None` once the window detached.
    keys: Option<ChildStdin>,
}

impl Person {
    /// Types `text` one key every `pace`, as a person does.
    pub fn types(&mut self, text: &str, pace: Duration) {
        for key in text.chars() {
            let mut bytes = [0; 4];
            self.press(key.encode_utf8(&mut bytes).as_bytes());
            thread::sleep(pace);
        }
    }

    /// Presses Enter.
    pub fn presses_enter(&mut self) {
        self.press(b"\r");
    }

    fn press(&mut self, key: &[u8]) {
        let keys = self.keys.as_mut().unwrap();
        keys.write_all(key).unwrap();
        keys.flush().unwrap();
    }
}

impl Drop for Person {
    /// Detaches the window, after the keys pressed before, and waits until it closed: `script`
    /// closes it at the end of its input, should Ctrl+\ no longer detach.
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

/// `path` quoted for a shell.
fn quoted(path: &OsStr) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', r"'\''"))
}

fn hc_binary() -> OsString {
    std::env::var_os("TEST_HC_BIN_PATH").unwrap_or_else(|| OsString::from("hc"))
}
