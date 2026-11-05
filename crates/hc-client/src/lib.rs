//! The reviewer's agent host under herdr-app: a client of herdr-core (`hc`), the core that runs
//! herdr-app's terminals and agents, behind herdr-client's [`AgentPort`] and [`AgentHost`].
//!
//! An agent is a terminal whose program reports its status with OSC 7501, or that runs Claude
//! Code in its foreground, whose session may report elsewhere, on a session its
//! `herdr-reviewer.session` label names; in the tab of herdr-app its `herdr.tab` label names.
//! The reviewer's hook sets the session label, since herdr-core keeps no agent session (ADR
//! 0016). An agent arrives when either appears, and leaves when neither holds. herdr-core keeps
//! no focus either: it is herdr-app's, on each of its screens, and the client follows a
//! screen's through its home's view and the layout herdr-core keeps for herdr-app.
//!
//! [`AgentPort`]: herdr_client::protocol::AgentPort
//! [`AgentHost`]: herdr_client::protocol::AgentHost

mod client;
mod events;
mod focus;
mod terminal;
#[cfg(test)]
mod test_screen;

pub use client::HcClient;
