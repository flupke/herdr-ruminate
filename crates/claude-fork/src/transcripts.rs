use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use notify::{RecursiveMode, Watcher};

const TURN_ENDS: &[&str] = &["end_turn", "stop_sequence", "max_tokens", "refusal"];

#[derive(Clone, Debug)]
pub struct Transcripts {
    projects: PathBuf,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct LastEntry {
    pub(crate) entry: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) ends_turn: bool,
}

impl Transcripts {
    pub fn at(projects: PathBuf) -> Self {
        Self { projects }
    }

    pub(crate) fn root(&self) -> &Path {
        &self.projects
    }

    pub fn find(&self, session: &str) -> Option<PathBuf> {
        let name = format!("{session}.jsonl");
        fs::read_dir(&self.projects)
            .ok()?
            .flatten()
            .map(|entry| entry.path().join(&name))
            .find(|path| path.is_file())
    }

    pub(crate) fn last_entry(&self, session: &str) -> Option<LastEntry> {
        let file = fs::File::open(self.find(session)?).ok()?;
        let mut last = LastEntry::default();
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            if !line.contains("\"type\":\"user\"") && !line.contains("\"type\":\"assistant\"") {
                continue;
            }
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            let kind = value.get("type").and_then(serde_json::Value::as_str);
            if !matches!(kind, Some("user" | "assistant")) {
                continue;
            }
            last.entry = value
                .get("uuid")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned);
            last.ends_turn = kind == Some("assistant")
                && value
                    .pointer("/message/stop_reason")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|reason| TURN_ENDS.contains(&reason));
            if let Some(model) = value
                .pointer("/message/model")
                .and_then(serde_json::Value::as_str)
                .filter(|model| kind == Some("assistant") && *model != "<synthetic>")
            {
                last.model = Some(model.to_owned());
            }
        }
        Some(last)
    }

    // Claude Code 2.1.329 records Code Mode in `{"type":"repl-mode","replMode":true}` entries,
    // the last one counting; a session without one runs without Code Mode.
    pub(crate) fn code_mode(&self, session: &str) -> bool {
        let Some(file) = self
            .find(session)
            .and_then(|path| fs::File::open(path).ok())
        else {
            return false;
        };
        BufReader::new(file)
            .lines()
            .map_while(Result::ok)
            .filter(|line| line.contains("\"type\":\"repl-mode\""))
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(&line).ok())
            .filter(|entry| {
                entry.get("type").and_then(serde_json::Value::as_str) == Some("repl-mode")
            })
            .last()
            .and_then(|entry| entry.get("replMode").and_then(serde_json::Value::as_bool))
            .unwrap_or(false)
    }

    pub(crate) fn turn_end(&self, session: &str, wait: Duration) -> Option<LastEntry> {
        let deadline = Instant::now() + wait;
        let (sender, changes) = mpsc::channel();
        let watcher = self.find(session).and_then(|path| {
            let directory = path.parent()?.to_owned();
            let mut watcher =
                notify::recommended_watcher(move |change: notify::Result<notify::Event>| {
                    if change
                        .as_ref()
                        .is_ok_and(|change| change.paths.contains(&path))
                    {
                        let _ = sender.send(change);
                    }
                })
                .ok()?;
            watcher
                .watch(&directory, RecursiveMode::NonRecursive)
                .ok()?;
            Some(watcher)
        });
        match watcher {
            Some(_watching) => self.turn_end_on(session, deadline, &changes),
            None => self.last_entry(session),
        }
    }

    fn turn_end_on(
        &self,
        session: &str,
        deadline: Instant,
        changes: &mpsc::Receiver<notify::Result<notify::Event>>,
    ) -> Option<LastEntry> {
        loop {
            let last = self.last_entry(session)?;
            let left = deadline.saturating_duration_since(Instant::now());
            if last.ends_turn || left.is_zero() {
                return Some(last);
            }
            if changes.recv_timeout(left).is_err() {
                return self.last_entry(session);
            }
        }
    }

    pub(crate) fn delete(&self, session: &str) {
        let Some(path) = self.find(session) else {
            return;
        };
        let _ = fs::remove_file(&path);
        let side = path.with_extension("");
        if side.is_dir() {
            let _ = fs::remove_dir_all(side);
        }
    }
}

#[cfg(test)]
#[path = "transcripts.tests.rs"]
mod tests;
