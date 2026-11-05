//! The review pane under herdr-core runs with the environment of its agent.
//!
//! herdr-core starts every terminal with its own server's environment, not with that of the
//! shell or the agent the pane opened beside: variables the user's shell exports, such as
//! `TYPESAFE_API_KEY` or `EDITOR`, are missing. Under Herdr, the pane took the environment of
//! the Herdr server the user started from their shell. So before anything else, the pane reads
//! the environment the agent's process started with, and runs its program again with it, once.
//! The pane's own variables of Herdr and of herdr-core stay, since they name its own terminal;
//! the agent's `HERDR_REVIEWER_*` settings, such as the MCP port, come with the rest.

use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::process::Command;

use hc_client::HcClient;
use herdr_client::protocol::AgentPort;

use super::host::{Arguments, HostKind, host_kind};

/// Variables the pane keeps its own of. The folders describe the agent's shell, not the pane;
/// the home, state and configuration folders keep the pane's state and `config.toml` where
/// every review pane of the user keeps them (ADR 0013), whatever runs in the focused terminal;
/// the temporary folders of a dev shell go with it, while the pane may outlive it.
const KEPT: [&str; 11] = [
    "PWD",
    "OLDPWD",
    "_",
    "HOME",
    "XDG_STATE_HOME",
    "XDG_CONFIG_HOME",
    "TMPDIR",
    "TMP",
    "TEMP",
    "TEMPDIR",
    "NIX_BUILD_TOP",
];

/// The variable the program run again finds set, so that it never runs again: the agent's
/// foreground process may change between two reads, or hold a variable the standard library
/// reads otherwise.
const ADOPTED: &str = "HERDR_REVIEWER_ADOPTED_AGENT_ENVIRONMENT";

/// Runs the reviewer's program again with the environment of the agent the pane opened beside,
/// under herdr-core, when that adds or changes a variable. Returns, leaving the pane's own
/// environment, under Herdr, without an agent, or when the agent's process cannot be read.
pub(super) fn adopt() {
    if host_kind(|name| env::var_os(name)) != Some(HostKind::HerdrCore)
        || env::var_os(ADOPTED).is_some()
    {
        return;
    }
    let Some(agent) = Arguments::parse(env::args().skip(1))
        .ok()
        .and_then(|arguments| arguments.focused_terminal)
    else {
        return;
    };
    let Some(pid) = HcClient::from_env()
        .and_then(|client| client.pane_process_info(&agent))
        .ok()
        .and_then(|info| info.foreground_process_group_id)
    else {
        return;
    };
    let Ok(inherited) = process_environment::read(pid) else {
        return;
    };
    let Some(mut environment) = adopted(env::vars_os(), inherited) else {
        return;
    };
    environment.insert(ADOPTED.into(), "1".into());
    let Ok(program) = env::current_exe() else {
        return;
    };
    let mut arguments = env::args_os();
    let mut command = Command::new(program);
    if let Some(name) = arguments.next() {
        command.arg0(name);
    }
    // Returns only when the program cannot run again; the pane goes on with its own environment.
    let _ = command.args(arguments).env_clear().envs(environment).exec();
}

/// The pane's `own` environment with each variable of the `agent`'s in place of its own, but
/// those in [`KEPT`]; `None` when that changes nothing, as in the program run again with it.
fn adopted(
    own: impl IntoIterator<Item = (OsString, OsString)>,
    agent: impl IntoIterator<Item = (OsString, OsString)>,
) -> Option<BTreeMap<OsString, OsString>> {
    let mut environment: BTreeMap<_, _> = own.into_iter().collect();
    // Of a variable it has twice, the agent reads the first, as `getenv` does.
    let mut first = BTreeMap::new();
    for (name, value) in agent {
        if name != ADOPTED && !KEPT.iter().any(|kept| name == *kept) {
            first.entry(name).or_insert(value);
        }
    }
    let mut changed = false;
    for (name, value) in first {
        if environment.get(&name) != Some(&value) {
            environment.insert(name, value);
            changed = true;
        }
    }
    changed.then_some(environment)
}

#[cfg(test)]
#[path = "agent_environment.tests.rs"]
mod tests;
