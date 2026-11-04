use std::os::unix::fs::PermissionsExt;

use super::*;

const GUARD: &str = "'/bin/reviewer-control' fork-guard";

fn guard_hooks() -> Value {
    serde_json::json!({
        "PreToolUse": [{"matcher": "*", "hooks": [{"type": "command", "command": GUARD}]}],
        "SessionStart": [
            {"hooks": [{"type": "command", "command": "echo herdr-reviewer-fork-guard >&2"}]}
        ],
    })
}

fn fork_settings(agent: &AgentSettings) -> Value {
    serde_json::from_str(&agent.with_guard(GUARD)).unwrap()
}

#[test]
fn a_fork_of_an_agent_without_settings_runs_its_guard_alone() {
    assert_eq!(
        fork_settings(&AgentSettings::default()),
        serde_json::json!({"disableAllHooks": false, "hooks": guard_hooks()})
    );
}

#[test]
fn a_fork_keeps_the_agent_s_settings_but_runs_its_guard_in_place_of_their_hooks() {
    let agent = serde_json::json!({
        "enableAllProjectMcpServers": true,
        "permissions": {"allow": ["mcp__herdr_bowtie"]},
        "hooks": {
            "PreToolUse": [{"matcher": "Bash", "hooks": [{"type": "command", "command": "allow-all"}]}],
            "Stop": [{"hooks": []}],
        },
    });

    let agent = AgentSettings::read(&agent.to_string(), Path::new("/nonexistent")).unwrap();

    assert_eq!(
        fork_settings(&agent),
        serde_json::json!({
            "enableAllProjectMcpServers": true,
            "permissions": {"allow": ["mcp__herdr_bowtie"]},
            "disableAllHooks": false,
            "hooks": guard_hooks(),
        })
    );
}

#[test]
fn the_agent_s_settings_are_read_from_their_file_from_the_agent_s_directory_or_inline() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("settings.json"),
        r#"{"env": {"FORCE_HYPERLINK": "1"}}"#,
    )
    .unwrap();
    let expected = serde_json::json!({
        "env": {"FORCE_HYPERLINK": "1"},
        "disableAllHooks": false,
        "hooks": guard_hooks(),
    });

    let from_file = AgentSettings::read("settings.json", directory.path()).unwrap();
    let inline = AgentSettings::read(
        r#" {"env": {"FORCE_HYPERLINK": "1"}}"#,
        Path::new("/nonexistent"),
    )
    .unwrap();

    assert_eq!(fork_settings(&from_file), expected);
    assert_eq!(fork_settings(&inline), expected);
}

#[test]
fn settings_a_fork_cannot_run_its_guard_with_take_no_fork() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("list.json"), "[1]").unwrap();
    let read = |value: &str| AgentSettings::read(value, directory.path());

    assert!(read("missing.json").unwrap_err().contains("cannot read"));
    assert!(read("list.json").unwrap_err().contains("not a JSON object"));
    // Inline settings may hold secrets, which the error leaves out.
    let inline = read(r#"{"env": {"TOKEN": "secret"}"#).unwrap_err();
    assert!(!inline.contains("secret"), "{inline}");
    assert!(read(r#"{"disableAllHooks": true}"#).is_err());
    assert!(read(r#"{"disableAllHooks": false}"#).is_ok());
}

#[test]
fn a_fork_s_settings_file_is_the_user_s_alone_and_goes_with_it() {
    let directory = tempfile::tempdir().unwrap();

    let file = SettingsFile::write_in(directory.path(), "fork-1", "{}").unwrap();
    let path = file.path().to_owned();

    assert_eq!(fs::read_to_string(&path).unwrap(), "{}");
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    drop(file);
    assert!(!path.exists());
}
