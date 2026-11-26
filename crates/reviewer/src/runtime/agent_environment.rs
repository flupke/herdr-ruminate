// herdr-core starts every terminal with its server's environment, not the agent's: variables
// the user's shell exports, such as `TYPESAFE_API_KEY` or `EDITOR`, are missing.

use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::process::Command;

use hc_client::HcClient;
use herdr_client::protocol::AgentPort;

use super::host::{Arguments, HostKind, host_kind};

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

const ADOPTED: &str = "HERDR_REVIEWER_ADOPTED_AGENT_ENVIRONMENT";

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
    let _ = command.args(arguments).env_clear().envs(environment).exec();
}

fn adopted(
    own: impl IntoIterator<Item = (OsString, OsString)>,
    agent: impl IntoIterator<Item = (OsString, OsString)>,
) -> Option<BTreeMap<OsString, OsString>> {
    let mut environment: BTreeMap<_, _> = own.into_iter().collect();
    // As `getenv` does, the agent reads the first of a variable it has twice.
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
