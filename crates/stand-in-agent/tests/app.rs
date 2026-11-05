//! What the reviewer's stand-in gives herdr-common's fake agent: the hooks of the reviewer's
//! plugin, and the turn's files read with the prompt. Being an integration test, it also has
//! Cargo build the program for every test run of the workspace, whose other tests run it too.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command as Process, Stdio};

use fake_agent::{Command, Event, Events, Role, variables};

/// The plugin's `reviewer-control`: it logs its arguments, a line each hook.
const CONTROL: &str =
    "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' \"$*\" >> \"$(dirname \"$0\")/hooks.log\"\n";

#[test]
fn the_stand_in_runs_the_plugin_s_hooks_and_reads_the_turn_s_files_with_its_prompt() {
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("events.sock");
    let events = Events::listen(&socket).unwrap();
    let control = directory.path().join("reviewer-control");
    fs::write(&control, CONTROL).unwrap();
    fs::set_permissions(&control, fs::Permissions::from_mode(0o755)).unwrap();
    let turn = directory.path().join("turn");
    fs::create_dir(&turn).unwrap();
    fs::write(turn.join("index.md"), "The turn's index.\n").unwrap();
    fs::write(turn.join("round.md"), "Explore round: r1\n").unwrap();
    fs::write(turn.join("turn.md"), "Explore request: q1\n").unwrap();
    let prompt = format!("Take the turn of {}/index.md", turn.display());
    let mut agent = Process::new(env!("CARGO_BIN_EXE_stand-in-agent"))
        .args(["--", &prompt])
        .env_remove("HERDR_PANE_ID")
        .env_remove("HERDR_SOCKET_PATH")
        .env_remove("HC_TERMINAL_ID")
        .env_remove("CLAUDE_CONFIG_DIR")
        .env(variables::EVENTS, &socket)
        .env(variables::NAME, "agent")
        .env(variables::HOOK, &control)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();

    let received = events.wait_for("the prompt", |reported| {
        matches!(reported.event, Event::PromptReceived { .. })
    });
    events.obey(&Role::agent("agent"), &Command::Exit);

    assert!(agent.wait().unwrap().success());
    assert_eq!(
        received.event,
        Event::PromptReceived {
            text: format!("{prompt}\nExplore round: r1\n\nExplore request: q1\n"),
        }
    );
    let calls = fs::read_to_string(directory.path().join("hooks.log")).unwrap();
    assert!(
        calls.lines().count() == 3 && calls.lines().all(|call| call == "agent-hook"),
        "SessionStart, UserPromptSubmit and SessionEnd, through `reviewer-control agent-hook`: \
         {calls}"
    );
}
