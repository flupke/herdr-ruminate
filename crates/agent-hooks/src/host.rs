use std::ffi::OsString;
use std::path::{Path, PathBuf};

use herdr_client::protocol::PaneId;

use crate::directory::HookDirectory;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HookPane {
    pub pane: PaneId,
    pub directory: HookDirectory,
    pub core: Option<PathBuf>,
}

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
    pub fn from_env() -> Option<Self> {
        Self::from_vars(|name| std::env::var_os(name))
    }

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

pub(crate) fn directory_from_env() -> Option<HookDirectory> {
    let set = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
    host_of(&set).map(|(_, _, directory)| directory)
}

// A Herdr started from a terminal of herdr-app may pass that terminal's herdr-core variables
// on to its panes, while its agents live in Herdr.
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
