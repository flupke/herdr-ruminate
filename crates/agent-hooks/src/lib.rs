//! The events that the hooks of an agent send to the reviewers: the reviewer confirms each
//! prompt it types through them, and run-ahead waits on them instead of reading the agent's
//! screen.
//!
//! The agent runs the reviewer's hooks (`claude-hooks` holds Claude Code's plugin, and
//! `codex-hooks` Codex's). Claude Code's hooks run `reviewer-control agent-hook`, which hands the
//! event, with the agent's pane, to every reviewer of the agent's host, a Herdr server or a
//! herdr-core ([`HookPane`], [`HookDirectory::tell`]). Codex's run `reviewer-control codex-hook`
//! in Codex's app-server daemon, which cannot tell the pane nor the host: it hands the event,
//! with the agent's session, to every reviewer of the user ([`HookRoot::ask`]). Each open
//! reviewer listens on a Unix socket of its own in its host's directory, under the user's runtime
//! directory, and follows the events of the panes and sessions it expects one from
//! ([`AgentHooks`]). A hook with no reviewer listening does nothing; one that cannot reach a
//! reviewer lets the agent go on.

mod address;
mod directory;
mod host;
mod listener;
mod wire;

pub use directory::{HookDirectory, HookRoot, HookedSession};
pub use host::HookPane;
pub use listener::{AgentHooks, Expectation, Heard, HookedAgent, OtherPrompts};
pub use wire::{AgentEvent, Report, SessionSource};
