//! Under Codex's app-server daemon, a hook runs in the daemon's process with the environment of
//! whichever terminal started it, so it cannot tell the agent's pane. A plugin's hook gets no
//! option of the user's, Codex runs it from a copy in its home, and never runs an untrusted hook.

mod app_server;
mod guard;
mod hook;
mod install;

pub use hook::{SUBCOMMAND, run_hook};
pub use install::{PLUGIN, codex_on_path, install, uninstall};
