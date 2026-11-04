//! The agent host the review pane runs in: Herdr, or herdr-core under herdr-app.

use std::ffi::OsString;
use std::sync::Arc;

use eyre::WrapErr;
use hc_client::HcClient;
use herdr_client::client::HerdrClient;
use herdr_client::protocol::{AgentHost, PaneId, PluginContext, TabId, WorkspaceId};

/// The option that names the terminal focused when the review pane opened, which herdr-app's
/// mod passes: herdr-core keeps no focus.
const FOCUSED_TERMINAL: &str = "--focused-terminal";

/// The host of the review pane, and where the pane runs in it.
pub(super) struct Host {
    pub(super) agents: Arc<dyn AgentHost>,
    /// The review pane's space (Herdr's workspace).
    pub(super) workspace_id: WorkspaceId,
    /// The review pane's tab: review prompts go only to an agent there.
    pub(super) tab_id: TabId,
    /// The pane focused when the review pane opened, the first agent it targets.
    pub(super) initial_agent: Option<PaneId>,
}

/// Which host runs the review pane.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HostKind {
    Herdr,
    /// herdr-core (`hc`), the core of herdr-app.
    HerdrCore,
}

/// Which host runs the review pane, by the variable each gives its terminals, which `var`
/// reads. Herdr's wins: a Herdr started from a terminal of herdr-app may pass that terminal's
/// herdr-core variables on to its panes, while its agents live in Herdr.
fn host_kind(var: impl Fn(&str) -> Option<OsString>) -> Option<HostKind> {
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
    /// The host of the review pane and its place, from the environment Herdr or herdr-core
    /// gives it and the `arguments` it runs with.
    pub(super) fn from_env(arguments: impl Iterator<Item = String>) -> eyre::Result<Self> {
        match host_kind(|name| std::env::var_os(name)) {
            Some(HostKind::Herdr) => Self::herdr(),
            Some(HostKind::HerdrCore) => Self::herdr_core(focused_terminal(arguments)?),
            None => Err(eyre::eyre!(
                "neither HERDR_SOCKET_PATH nor HC_TERMINAL_SOCKET is set: the reviewer runs in a \
                 pane of Herdr or a terminal of herdr-app"
            )),
        }
    }

    /// Herdr gives its plugin pane its workspace, its tab and the focused pane in variables.
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

    /// herdr-app shows a terminal in the space and the tab its labels name.
    fn herdr_core(initial_agent: Option<PaneId>) -> eyre::Result<Self> {
        let client = HcClient::from_env()?;
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
            initial_agent,
        })
    }
}

/// The terminal named by `--focused-terminal <terminal>` among the reviewer's `arguments`, its
/// program name left out.
fn focused_terminal(mut arguments: impl Iterator<Item = String>) -> eyre::Result<Option<PaneId>> {
    let usage = || eyre::eyre!("usage: reviewer [{FOCUSED_TERMINAL} <terminal>]");
    let mut focused = None;
    while let Some(argument) = arguments.next() {
        if argument != FOCUSED_TERMINAL {
            return Err(usage());
        }
        focused = Some(PaneId(arguments.next().ok_or_else(usage)?));
    }
    Ok(focused)
}

#[cfg(test)]
#[path = "host.tests.rs"]
mod tests;
