use agent_fork::ForkCommand;

use super::*;
use crate::settings::AgentSettings;

fn agent_in(configuration: &std::path::Path) -> PaneClaude {
    PaneClaude {
        command: ForkCommand {
            program: "claude".into(),
            arguments: Vec::new(),
            directory: "/work".into(),
            environment: vec![("CLAUDE_CONFIG_DIR".into(), configuration.into())],
        },
        model: None,
        settings: AgentSettings::default(),
    }
}

#[test]
fn a_session_started_in_code_mode_is_one_no_fork_can_resume() {
    let configuration = tempfile::tempdir().unwrap();
    let project = configuration.path().join("projects/-work");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(
        project.join("code-mode.jsonl"),
        concat!(
            r#"{"type":"mode","mode":"normal","sessionId":"code-mode"}"#,
            "\n",
            r#"{"type":"repl-mode","replMode":true,"sessionId":"code-mode"}"#,
            "\n",
            r#"{"type":"user","uuid":"u1","message":{"role":"user"}}"#,
            "\n",
        ),
    )
    .unwrap();
    std::fs::write(
        project.join("plain.jsonl"),
        concat!(
            r#"{"type":"mode","mode":"normal","sessionId":"plain"}"#,
            "\n",
            r#"{"type":"user","uuid":"u1","message":{"role":"user"}}"#,
            "\n",
        ),
    )
    .unwrap();
    let agent = agent_in(configuration.path());

    let why = unresumable(&agent, "code-mode").unwrap();

    assert!(why.contains("Code Mode"), "{why}");
    assert_eq!(unresumable(&agent, "plain"), None);
}
