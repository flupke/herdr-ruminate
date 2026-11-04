//! The reviewer's agent host under herdr-app: a client of herdr-core (`hc`), the core that runs
//! herdr-app's terminals and agents, behind herdr-client's [`AgentPort`] and [`AgentHost`].
//!
//! An agent is a terminal whose program reports its status with OSC 7501, in the tab of
//! herdr-app its `herdr.tab` label names, on the session its `herdr-reviewer.session` label
//! names, which the reviewer sets there since herdr-core keeps no agent session (ADR 0016).
//! An agent's arrival or departure is the appearance or the end of its status. herdr-core keeps
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
