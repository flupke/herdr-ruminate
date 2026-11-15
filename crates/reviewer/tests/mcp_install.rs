use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::process::Command;

use test_case::test_case;

#[test_case(false; "default_user_paths")]
#[test_case(true; "custom_user_paths")]
fn installation_registers_selected_clients_outside_a_repository(custom_paths: bool) {
    let directory = tempfile::tempdir().unwrap();
    let working_directory = directory.path().join("working-directory");
    let user_home = directory.path().join("home");
    fs::create_dir(&working_directory).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_reviewer-control"));
    command
        .arg("mcp-install")
        .current_dir(&working_directory)
        .env("HOME", &user_home)
        .env("PATH", path_with_codex(directory.path()))
        .env_remove("CODEX_HOME")
        .env_remove("CLAUDE_CONFIG_DIR");
    let (codex, claude) = if custom_paths {
        let codex = directory.path().join("codex");
        let claude = directory.path().join("claude");
        command
            .env("CODEX_HOME", &codex)
            .env("CLAUDE_CONFIG_DIR", &claude);
        (codex.join("config.toml"), claude.join(".claude.json"))
    } else {
        (
            user_home.join(".codex/config.toml"),
            user_home.join(".claude.json"),
        )
    };
    let mut selected = Command::new(command.get_program());
    selected
        .args(command.get_args())
        .arg("codex")
        .current_dir(&working_directory);
    for (key, value) in command.get_envs() {
        if let Some(value) = value {
            selected.env(key, value);
        } else {
            selected.env_remove(key);
        }
    }
    let output = selected.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !claude.exists(),
        "a selected installation must leave the other client alone"
    );
    let original_codex = fs::read_to_string(&codex).unwrap();
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let original_claude = fs::read_to_string(&claude).unwrap();
    assert!(original_claude.contains("herdr_reviewer"));
    assert!(original_codex.contains("herdr_reviewer"));
    assert!(command.output().unwrap().status.success());
    assert_eq!(fs::read_to_string(codex).unwrap(), original_codex);
    assert_eq!(fs::read_to_string(claude).unwrap(), original_claude);
    assert_eq!(fs::read_dir(working_directory).unwrap().count(), 0);
}

/// A `PATH` of a directory in `directory` that holds a `codex`.
fn path_with_codex(directory: &Path) -> std::path::PathBuf {
    let path = directory.join("path");
    fs::create_dir(&path).unwrap();
    fs::write(path.join("codex"), "#!/bin/sh\n").unwrap();
    fs::set_permissions(path.join("codex"), fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[test]
fn without_codex_on_the_path_only_a_named_codex_is_registered() {
    let directory = tempfile::tempdir().unwrap();
    let user_home = directory.path().join("home");
    let empty_path = directory.path().join("path");
    fs::create_dir(&empty_path).unwrap();
    let install = |clients: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_reviewer-control"))
            .arg("mcp-install")
            .args(clients)
            .env("HOME", &user_home)
            .env("PATH", &empty_path)
            .env_remove("CODEX_HOME")
            .env_remove("CLAUDE_CONFIG_DIR")
            .output()
            .unwrap()
    };
    let codex = user_home.join(".codex/config.toml");

    let output = install(&[]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(
        stderr.contains("no `codex` on PATH, skipped its MCP registration"),
        "{stderr}"
    );
    assert!(
        fs::read_to_string(user_home.join(".claude.json"))
            .unwrap()
            .contains("herdr_reviewer")
    );
    assert!(!user_home.join(".codex").exists());

    let output = install(&["codex"]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        fs::read_to_string(codex)
            .unwrap()
            .contains("herdr_reviewer")
    );
}
