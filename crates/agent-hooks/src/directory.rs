use std::fmt::Write as _;
use std::fs;
use std::io::{self, BufRead as _, BufReader, Write as _};
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use agent_fork::ProcessStamp;
use herdr_client::protocol::PaneId;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::wire::{Answer, Report};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HookDirectory(PathBuf);

// Codex runs its hooks in its app-server daemon, with the environment of whichever terminal
// started it: they cannot tell their host or pane.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HookRoot(PathBuf);

const CODEX_SESSIONS: &str = "codex";

const CODEX_FORKS: &str = "codex-forks";

const SOCKET: &str = "sock";

const RECORD: &str = "pane";

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct HookedSession {
    pub session: String,
    pub process: ProcessStamp,
}

const TELL_LIMIT: Duration = Duration::from_millis(200);

impl HookDirectory {
    pub fn for_server(runtime: &Path, socket: &Path) -> Self {
        let digest = Sha256::digest(socket.as_os_str().as_encoded_bytes());
        Self(HookRoot::new(runtime).0.join(hex(&digest[..6])))
    }

    pub fn root(&self) -> HookRoot {
        HookRoot(self.0.parent().unwrap_or(&self.0).to_owned())
    }

    pub fn from_env() -> Option<Self> {
        crate::host::directory_from_env()
    }

    pub fn tell(&self, report: &Report) {
        let Some(line) = line(report) else {
            return;
        };
        for socket in self.sockets() {
            let _ = send(&socket, &line, TELL_LIMIT);
        }
    }

    pub fn ask(&self, report: &Report, limit: Duration) -> Option<String> {
        ask_each(&self.sockets(), report, limit)
    }

    // A Claude Code started from the shell of an agent's pane inherits that pane's Herdr or
    // herdr-core variables, and runs the hooks too.
    pub fn record(&self, pane: &PaneId, started: &HookedSession) -> io::Result<()> {
        self.create()?;
        for (path, earlier) in self.records(pane) {
            if !earlier.process.is_running() {
                let _ = fs::remove_file(path);
            }
        }
        let path = self.0.join(format!(
            "{}{}.{RECORD}",
            record_prefix(pane),
            started.process.pid
        ));
        let written = path.with_extension(format!("{}.new", std::process::id()));
        fs::write(&written, serde_json::to_vec(started)?)?;
        fs::rename(&written, &path)
    }

    pub fn hooked_sessions(&self, pane: &PaneId) -> Vec<HookedSession> {
        self.records(pane)
            .into_iter()
            .map(|(_, started)| started)
            .collect()
    }

    fn records(&self, pane: &PaneId) -> Vec<(PathBuf, HookedSession)> {
        let prefix = record_prefix(pane);
        let Ok(entries) = fs::read_dir(&self.0) else {
            return Vec::new();
        };
        entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == RECORD)
                    && path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with(&prefix))
            })
            .filter_map(|path| {
                let started = serde_json::from_slice(&fs::read(&path).ok()?).ok()?;
                Some((path, started))
            })
            .collect()
    }

    pub(crate) fn create(&self) -> io::Result<()> {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.0)
    }

    pub(crate) fn socket(&self, process: u32, number: u64) -> PathBuf {
        self.0.join(format!("{process}-{number}.{SOCKET}"))
    }

    pub(crate) fn sockets(&self) -> Vec<PathBuf> {
        let Ok(entries) = fs::read_dir(&self.0) else {
            return Vec::new();
        };
        entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == SOCKET)
            })
            .collect()
    }

    pub(crate) fn remove_stale_sockets(&self) {
        for socket in self.sockets() {
            let process = socket
                .file_stem()
                .and_then(|stem| stem.to_str())
                .and_then(|stem| stem.split_once('-'))
                .and_then(|(process, _)| process.parse::<i32>().ok())
                .and_then(rustix::process::Pid::from_raw);
            if process.is_some_and(|process| {
                rustix::process::test_kill_process(process) == Err(rustix::io::Errno::SRCH)
            }) {
                let _ = fs::remove_file(socket);
            }
        }
    }
}

impl HookRoot {
    pub(crate) fn new(runtime: &Path) -> Self {
        Self(runtime.join("herdr-reviewer"))
    }

    pub fn from_env() -> Option<Self> {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR").filter(|path| !path.is_empty())?;
        Some(Self::new(Path::new(&runtime)))
    }

    pub fn ask(&self, report: &Report, limit: Duration) -> Option<String> {
        let Ok(entries) = fs::read_dir(&self.0) else {
            return None;
        };
        let sockets: Vec<PathBuf> = entries
            .flatten()
            .flat_map(|entry| HookDirectory(entry.path()).sockets())
            .collect();
        ask_each(&sockets, report, limit)
    }

    // A Codex session keeps the hooks it was loaded with: only its hook can tell that it runs
    // them.
    pub fn record_codex_session(&self, session: &str) -> io::Result<()> {
        let sessions = self.0.join(CODEX_SESSIONS);
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&sessions)?;
        let record = sessions.join(hex(session.as_bytes()));
        if record.exists() {
            return Ok(());
        }
        fs::write(record, "")
    }

    pub(crate) fn runs_codex_hook(&self, session: &str) -> bool {
        self.0
            .join(CODEX_SESSIONS)
            .join(hex(session.as_bytes()))
            .exists()
    }

    pub fn mark_codex_fork(&self, session: &str) -> io::Result<()> {
        let forks = self.0.join(CODEX_FORKS);
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&forks)?;
        fs::write(forks.join(hex(session.as_bytes())), "")
    }

    pub fn unmark_codex_fork(&self, session: &str) -> io::Result<()> {
        match fs::remove_file(self.0.join(CODEX_FORKS).join(hex(session.as_bytes()))) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    }

    pub fn is_codex_fork(&self, session: &str) -> bool {
        self.0
            .join(CODEX_FORKS)
            .join(hex(session.as_bytes()))
            .exists()
    }
}

fn record_prefix(pane: &PaneId) -> String {
    format!("{}-", hex(pane.0.as_bytes()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}

fn line(report: &Report) -> Option<Vec<u8>> {
    let mut line = serde_json::to_vec(report).ok()?;
    line.push(b'\n');
    Some(line)
}

fn ask_each(sockets: &[PathBuf], report: &Report, limit: Duration) -> Option<String> {
    let line = line(report)?;
    let deadline = Instant::now() + limit;
    sockets
        .iter()
        .find_map(|socket| ask(socket, &line, deadline).ok().flatten())
}

fn send(socket: &Path, line: &[u8], limit: Duration) -> io::Result<std::os::unix::net::UnixStream> {
    let mut stream = crate::address::connect(socket)?;
    stream.set_write_timeout(Some(limit))?;
    stream.write_all(line)?;
    Ok(stream)
}

fn ask(socket: &Path, line: &[u8], deadline: Instant) -> io::Result<Option<String>> {
    let left = || {
        Some(deadline.saturating_duration_since(Instant::now()))
            .filter(|left| !left.is_zero())
            .ok_or(io::ErrorKind::TimedOut)
    };
    let stream = send(socket, line, left()?)?;
    stream.set_read_timeout(Some(left()?))?;
    let mut answer = String::new();
    BufReader::new(stream).read_line(&mut answer)?;
    Ok(serde_json::from_str::<Answer>(&answer)?.block)
}
