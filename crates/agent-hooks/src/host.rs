//! The host whose agents' hooks reach a reviewer: Herdr, or herdr-core under herdr-app, as the
//! variables each gives the programs of its panes name it. Herdr's win, as the review pane picks
//! its host: a Herdr started from a terminal of herdr-app may pass that terminal's herdr-core
//! variables on to its panes, while its agents live in Herdr.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use herdr_client::protocol::PaneId;

use crate::directory::HookDirectory;

/// The pane of the agent a hook runs for, and where the reviewers of its host listen.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HookPane {
    /// Herdr's pane, or herdr-core's terminal.
    pub pane: PaneId,
    pub directory: HookDirectory,
    /// herdr-core's socket, when the pane is a terminal of herdr-core: the hook reports the
    /// agent's session there (ADR 0016), where Herdr's own hook reports it under Herdr.
    pub core: Option<PathBuf>,
}

/// The variables of each host: its socket, and the pane of the program that reads them.
struct Variables {
    socket: &'static str,
    pane: &'static str,
    core: bool,
}

const HERDR: Variables = Variables {
    socket: "HERDR_SOCKET_PATH",
    pane: "HERDR_PANE_ID",
    core: false,
};

const CORE: Variables = Variables {
    socket: "HC_TERMINAL_SOCKET",
    pane: "HC_TERMINAL_ID",
    core: true,
};

impl HookPane {
    /// The pane this process runs in, from the environment its host gives it; `None` outside
    /// one, or without `XDG_RUNTIME_DIR`.
    pub fn from_env() -> Option<Self> {
        Self::from_vars(|name| std::env::var_os(name))
    }

    /// The pane that the variables `var` reads name.
    fn from_vars(var: impl Fn(&str) -> Option<OsString>) -> Option<Self> {
        let set = |name: &str| var(name).filter(|value| !value.is_empty());
        let (host, socket, directory) = host_of(&set)?;
        Some(Self {
            pane: PaneId(set(host.pane)?.into_string().ok()?),
            directory,
            core: host.core.then_some(socket),
        })
    }
}

/// The directory of the host this process runs under, from `XDG_RUNTIME_DIR` and the host's
/// socket; `None` without either.
pub(crate) fn directory_from_env() -> Option<HookDirectory> {
    let set = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
    host_of(&set).map(|(_, _, directory)| directory)
}

/// The host whose socket `set` names, Herdr's first, that socket, and the host's directory
/// under the runtime directory `set` names.
fn host_of(
    set: &impl Fn(&str) -> Option<OsString>,
) -> Option<(&'static Variables, PathBuf, HookDirectory)> {
    let (host, socket) = [&HERDR, &CORE]
        .into_iter()
        .find_map(|host| Some((host, PathBuf::from(set(host.socket)?))))?;
    let directory = HookDirectory::for_server(Path::new(&set("XDG_RUNTIME_DIR")?), &socket);
    Some((host, socket, directory))
}

#[cfg(test)]
#[path = "host.tests.rs"]
mod tests;
