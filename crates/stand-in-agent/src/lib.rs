//! The stand-in agent that the reviewer's tests and `make vision` sessions run in a Herdr pane:
//! herdr-common's fake agent ([`fake_agent`]), run with the reviewer's [`APP`] by this crate's
//! program. It plays Claude Code or Codex in its pane, a fork of Claude Code's session when
//! run-ahead starts one, and, run by the hooks of a real Claude Code
//! ([`fake_agent::observing_hooks`]), the reporter of what that agent does. The variables it
//! reads are [`fake_agent::variables`]; its event socket is [`fake_agent::Events`].
//!
//! This library holds what a test or a vision session needs beside `fake_agent`: where Cargo
//! builds the program, and how a test installs it under an agent's name.

use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

/// What the reviewer's stand-in does its own way: the hooks of the reviewer's Claude Code plugin
/// run through the `reviewer-control` that [`fake_agent::variables::HOOK`] names, and the agent
/// and its forks read the turn's files with the prompt that names them, as it arrives.
pub const APP: fake_agent::App = fake_agent::App {
    hook_arguments: &[claude_hooks::SUBCOMMAND],
    read_prompt: review_test_support::turn::with_turn,
};

/// The program's name, as Cargo builds it.
const PROGRAM: &str = "stand-in-agent";

/// The stand-in that Cargo built in the profile directory `profile`, such as `target/debug`.
pub fn program(profile: &Path) -> Result<PathBuf, String> {
    let program = profile.join(PROGRAM);
    if program.is_file() {
        Ok(program)
    } else {
        Err(format!(
            "{} is not built: run `cargo build -p {PROGRAM}`, as `make test` does",
            program.display()
        ))
    }
}

/// Installs `program` as `agent`, a link whose name Herdr detects the agent by, rather than a
/// copy: every test that runs at once installs it.
pub fn install(program: &Path, agent: &Path) -> std::io::Result<()> {
    symlink(program, agent)
}
