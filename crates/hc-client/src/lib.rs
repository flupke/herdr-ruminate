//! herdr-core keeps no agent session and no focus: the reviewer's hook sets a terminal's
//! session label, and the focus is herdr-app's, on each of its screens.

mod client;
mod events;
mod focus;
mod terminal;
#[cfg(test)]
mod test_screen;

pub use client::HcClient;
