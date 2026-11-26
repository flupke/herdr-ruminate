use std::path::Path;

use agent_hooks::HookDirectory;

use super::*;

fn codex_agent() -> Agent {
    serde_json::from_value(json!({
        "pane_id": "w:p1",
        "tab_id": "tab",
        "workspace_id": "workspace",
        "agent": "codex",
        "agent_status": "idle",
    }))
    .unwrap()
}

#[test]
fn an_agent_whose_session_the_reviewer_does_not_know_yet_is_not_taken_for_one_without_hooks() {
    let runtime = tempfile::tempdir().unwrap();
    let hooks = Arc::new(
        AgentHooks::listen(&HookDirectory::for_server(
            runtime.path(),
            Path::new("/run/herdr.sock"),
        ))
        .unwrap(),
    );
    let herdr = herdr_client::client::HerdrClient::new(
        runtime.path().join("no-herdr.sock"),
        "test".into(),
        runtime.path().to_owned(),
    );
    let forks = CodexForks::new(
        Arc::new(herdr),
        RunAheadLog::default(),
        ForkWaits::default(),
        Some(Arc::clone(&hooks)),
    );
    let agent = codex_agent();

    assert_eq!(forks.unhooked(&agent), None);

    hooks.name_session(&agent.pane_id, "thread-without-the-hook");
    assert!(forks.unhooked(&agent).is_some());
}
