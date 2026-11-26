use std::path::PathBuf;

use codex_daemon::Daemon;
use review_run_ahead::HostFields;
use serde_json::Value;

const DAEMON: &str = "codex_daemon";
const TURN: &str = "codex_turn";
const SETTINGS: &str = "codex_settings";

pub(crate) fn point(daemon: &Daemon) -> HostFields {
    let mut fields = HostFields::default();
    fields.set(DAEMON, daemon.socket());
    fields
}

pub(crate) fn fork(daemon: &Daemon, settings: &Value) -> HostFields {
    let mut fields = point(daemon);
    fields.set(SETTINGS, settings);
    fields
}

pub(crate) fn started(turn: &str) -> HostFields {
    let mut fields = HostFields::default();
    fields.set(TURN, turn);
    fields
}

pub(crate) fn daemon(fields: &HostFields) -> Option<Daemon> {
    fields.get::<PathBuf>(DAEMON).map(Daemon::at)
}

pub fn turn(fields: &HostFields) -> Option<String> {
    fields.get(TURN)
}

pub(crate) fn settings(fields: &HostFields) -> Option<Value> {
    fields.get(SETTINGS)
}

pub fn owns(fields: &HostFields) -> bool {
    fields.has(DAEMON)
}
