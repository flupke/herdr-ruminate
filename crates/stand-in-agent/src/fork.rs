//! A fork of the stand-in: it records its arguments, settings, environment and prompt in its
//! directory, copies its parent's transcript, runs its settings' `SessionStart` hooks and reports
//! them as Claude Code does, then works until it is told to end its turn or to crash, or the
//! reviewer stops it. Once told to submit its turn, its output says that its submit has its
//! answer. It reports each step on the event socket, and takes its commands there.

use std::fs;
use std::io::{self, Read as _, Write as _};
use std::path::PathBuf;

use review_test_support::stand_in::{StandInCommand, StandInConnection, StandInEvent, StandInRole};
use stand_in_agent::{StandInDirectory, variables};

use crate::transcript::own_configuration;

/// Runs the fork that Claude Code's `arguments` start.
pub(crate) fn run(arguments: &[String]) {
    let value = |name: &str| {
        let at = arguments
            .iter()
            .position(|argument| argument == name)
            .unwrap_or_else(|| panic!("a fork takes {name}"));
        arguments[at + 1].clone()
    };
    let directory = StandInDirectory::new(PathBuf::from(
        std::env::var_os(variables::DIRECTORY).expect("the stand-in's directory"),
    ));
    let (session, parent) = (value("--session-id"), value("--resume"));
    let (report, commands) = StandInConnection::from_env(StandInRole::Fork {
        session: session.clone(),
    })
    .expect("the event socket");
    let mut prompt = String::new();
    io::stdin().read_to_string(&mut prompt).unwrap();
    let configuration = own_configuration().expect("Claude Code's configuration directory");
    let mut history =
        fs::read_to_string(stand_in_agent::transcript(&configuration, &parent)).unwrap();
    history.push_str("{\"type\":\"user\",\"uuid\":\"fork-prompt\"}\n");
    fs::write(
        stand_in_agent::transcript(&configuration, &session),
        history,
    )
    .unwrap();
    let names: Vec<_> = std::env::vars_os()
        .map(|(name, _)| name.to_string_lossy().into_owned())
        .collect();
    let record = |kind: &str, content: &str| {
        let path = directory.fork_record(&session, kind);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    };
    record("args", &arguments.join("\n"));
    record("env", &names.join("\n"));
    // Claude Code takes the path of a settings file, as the reviewer gives its forks.
    let settings = arguments
        .iter()
        .position(|argument| argument == "--settings")
        .map(|at| fs::read_to_string(&arguments[at + 1]).expect("a fork's settings file"))
        .unwrap_or_default();
    record("settings", &settings);
    // The fork reads the turn's files as it reads the prompt that names them.
    let prompt = review_test_support::turn::with_turn(&prompt);
    record("prompt", &prompt);
    report.report(&StandInEvent::ForkStarted { parent, prompt });
    let usage = r#"{"input_tokens":3,"cache_creation_input_tokens":20,"cache_read_input_tokens":1000,"output_tokens":7}"#;
    for stderr in session_start_hooks(&settings) {
        let response = serde_json::json!({"type": "system", "subtype": "hook_response",
            "hook_name": "SessionStart:fork", "stderr": stderr});
        println!("{response}");
    }
    println!(r#"{{"type":"system","subtype":"init","session_id":"{session}"}}"#);
    println!(r#"{{"type":"assistant","message":{{"id":"m1","usage":{usage}}}}}"#);
    io::stdout().flush().unwrap();
    let mut answered = false;
    // A closed socket means that nobody follows the fork any more.
    for command in commands {
        match command {
            StandInCommand::End => break,
            StandInCommand::Crash => {
                eprintln!("stand-in fork: crashed as it was told to");
                std::process::exit(1);
            }
            StandInCommand::Submit if !answered => {
                answered = true;
                println!(
                    r#"{{"type":"assistant","message":{{"id":"m2","content":[{{"type":"tool_use","id":"submit","name":"mcp__herdr_reviewer__submit_question","input":{{}}}}],"usage":{usage}}}}}"#
                );
                println!(
                    r#"{{"type":"user","message":{{"content":[{{"type":"tool_result","tool_use_id":"submit"}}]}}}}"#
                );
                io::stdout().flush().unwrap();
                report.report(&StandInEvent::ForkSubmitted);
            }
            StandInCommand::Submit => {}
            command => panic!("a fork got the agent's command {command:?}"),
        }
    }
    // The turn ends in the fork's transcript, as Claude Code's does.
    let mut transcript = fs::File::options()
        .append(true)
        .open(stand_in_agent::transcript(&configuration, &session))
        .unwrap();
    writeln!(
        transcript,
        r#"{{"type":"assistant","uuid":"fork-answer","message":{{"model":"stand-in-model","stop_reason":"end_turn"}}}}"#
    )
    .unwrap();
    println!(r#"{{"type":"result","subtype":"success","is_error":false}}"#);
    io::stdout().flush().unwrap();
    report.report(&StandInEvent::ForkFinished);
}

/// The standard error of each `SessionStart` hook of `settings`, run as Claude Code runs them;
/// none for settings Claude Code skips as a whole, such as those whose `permissions.allow` is
/// not a list.
fn session_start_hooks(settings: &str) -> Vec<String> {
    let Ok(settings) = serde_json::from_str::<serde_json::Value>(settings) else {
        return Vec::new();
    };
    if settings
        .pointer("/permissions/allow")
        .is_some_and(|allow| !allow.is_array())
    {
        return Vec::new();
    }
    settings
        .pointer("/hooks/SessionStart")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|matcher| matcher.get("hooks")?.as_array())
        .flatten()
        .filter_map(|hook| hook.get("command")?.as_str())
        .map(|command| {
            let output = std::process::Command::new("sh")
                .args(["-c", command])
                .output()
                .unwrap();
            String::from_utf8_lossy(&output.stderr).into_owned()
        })
        .collect()
}
