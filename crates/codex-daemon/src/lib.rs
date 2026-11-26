//! The Codex TUI runs its sessions on an app-server daemon that every TUI of the Codex home
//! shares, and runs every session's hooks with the environment of the TUI that started it. The
//! daemon tells every client that joins a session what that session does.

mod daemon;
mod follow;
mod rpc;
mod wire;

pub use daemon::{Daemon, NoDaemon};
pub use follow::{Follow, SessionEvent, SessionStatus, TurnEnd};
pub use rpc::{Incoming, Replier, Rpc};
