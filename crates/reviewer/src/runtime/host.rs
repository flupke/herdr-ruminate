use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;

use eyre::WrapErr;
use hc_client::HcClient;
use herdr_app_focus::AppHome;
use herdr_client::client::HerdrClient;
use herdr_client::protocol::{AgentHost, PaneId, PluginContext, TabId, WorkspaceId};

// herdr-core keeps no focus: herdr-app's mod passes the terminal focused when the pane opened.
const FOCUSED_TERMINAL: &str = "--focused-terminal";
// A terminal's `HERDR_APP_HOME` is the one herdr-core started with, not its window's: herdr-app's
// mod passes the window's.
const HERDR_APP_HOME: &str = "--herdr-app-home";

#[derive(Debug, Default, Eq, PartialEq)]
pub(super) struct Arguments {
    pub(super) focused_terminal: Option<PaneId>,
    herdr_app_home: Option<PathBuf>,
}

pub(super) struct Host {
    pub(super) agents: Arc<dyn AgentHost>,
    pub(super) workspace_id: WorkspaceId,
    pub(super) tab_id: TabId,
    pub(super) initial_agent: Option<PaneId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum HostKind {
    Herdr,
    HerdrCore,
}

// A Herdr started from a herdr-app terminal may pass that terminal's herdr-core variables on to
// its panes, while its agents live in Herdr.
pub(super) fn host_kind(var: impl Fn(&str) -> Option<OsString>) -> Option<HostKind> {
    let set = |name: &str| var(name).is_some_and(|value| !value.is_empty());
    if set("HERDR_SOCKET_PATH") {
        Some(HostKind::Herdr)
    } else if set("HC_TERMINAL_SOCKET") {
        Some(HostKind::HerdrCore)
    } else {
        None
    }
}

impl Host {
    pub(super) fn from_env(arguments: impl Iterator<Item = String>) -> eyre::Result<Self> {
        match host_kind(|name| std::env::var_os(name)) {
            Some(HostKind::Herdr) => Self::herdr(),
            Some(HostKind::HerdrCore) => Self::herdr_core(Arguments::parse(arguments)?),
            None => Err(eyre::eyre!(
                "neither HERDR_SOCKET_PATH nor HC_TERMINAL_SOCKET is set: the reviewer runs in a \
                 pane of Herdr or a terminal of herdr-app"
            )),
        }
    }

    fn herdr() -> eyre::Result<Self> {
        let var = |name: &str| std::env::var(name).wrap_err_with(|| format!("{name} is not set"));
        let context: PluginContext = serde_json::from_str(&var("HERDR_PLUGIN_CONTEXT_JSON")?)?;
        Ok(Self {
            agents: Arc::new(HerdrClient::from_env()?),
            workspace_id: WorkspaceId(var("HERDR_WORKSPACE_ID")?),
            tab_id: TabId(var("HERDR_TAB_ID")?),
            initial_agent: context.focused_pane_id,
        })
    }

    // herdr-app shows a terminal in the space and the tab its labels name.
    fn herdr_core(arguments: Arguments) -> eyre::Result<Self> {
        let mut client = HcClient::from_env()?;
        if let Some(home) = focus_home(arguments.herdr_app_home, |name| std::env::var_os(name)) {
            client = client.following_focus(home);
        }
        let terminal =
            PaneId(std::env::var("HC_TERMINAL_ID").wrap_err("HC_TERMINAL_ID is not set")?);
        let (workspace_id, tab_id) = client.place(&terminal)?.ok_or_else(|| {
            eyre::eyre!(
                "terminal {} has no herdr.space and herdr.tab labels: herdr-app does not show it",
                terminal.0
            )
        })?;
        Ok(Self {
            agents: Arc::new(client),
            workspace_id,
            tab_id,
            initial_agent: arguments.focused_terminal,
        })
    }
}

fn focus_home(named: Option<PathBuf>, var: impl Fn(&str) -> Option<OsString>) -> Option<AppHome> {
    named.map(AppHome::new).or_else(|| {
        var("HOME")
            .filter(|home| !home.is_empty())
            .map(|home| AppHome::default_in(home.as_ref()))
    })
}

impl Arguments {
    pub(super) fn parse(mut arguments: impl Iterator<Item = String>) -> eyre::Result<Self> {
        let usage = || {
            eyre::eyre!(
                "usage: reviewer [{FOCUSED_TERMINAL} <terminal>] [{HERDR_APP_HOME} <folder>]"
            )
        };
        let mut parsed = Self::default();
        while let Some(option) = arguments.next() {
            let mut value = || arguments.next().ok_or_else(usage);
            match option.as_str() {
                FOCUSED_TERMINAL => parsed.focused_terminal = Some(PaneId(value()?)),
                HERDR_APP_HOME => parsed.herdr_app_home = Some(value()?.into()),
                _ => return Err(usage()),
            }
        }
        Ok(parsed)
    }
}

#[cfg(test)]
#[path = "host.tests.rs"]
mod tests;
