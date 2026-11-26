use std::path::Path;

use herdr_client::memory::InMemoryAgents;
use herdr_client::protocol::Agent;

use super::{Effort, ExploreModels, ModelEffort};

const AGENT_FILE: &str = "---\nname: bt-self-review\ndescription: \"A self-review of a change, on the reviewer settings: claude-fable-5-2 at max effort.\"\nmodel: claude-fable-5-2\neffort: max\n---\n\nDo the review.\n";

fn agent(kind: &str) -> Agent {
    serde_json::from_value(serde_json::json!({
        "pane_id": "pane", "tab_id": "tab", "workspace_id": "workspace",
        "agent": kind, "agent_status": "idle",
    }))
    .unwrap()
}

fn prompts_folder(text: &str) -> tempfile::TempDir {
    let folder = tempfile::tempdir().unwrap();
    let agents = folder.path().join(".claude/agents");
    std::fs::create_dir_all(&agents).unwrap();
    std::fs::write(agents.join("bt-self-review.md"), text).unwrap();
    folder
}

fn fable() -> ModelEffort {
    ModelEffort {
        model: "claude-fable-5-2".into(),
        effort: Effort::Max,
    }
}

fn on_fable(worker: Option<(&str, Effort)>) -> ExploreModels {
    ExploreModels {
        round: fable(),
        worker: worker.map(|(model, effort)| ModelEffort {
            model: model.into(),
            effort,
        }),
    }
}

#[test]
fn a_claude_code_agent_started_by_herdr_bowtie_runs_the_round_on_the_reviewer_s_model_and_implements_on_its_own()
 {
    let folder = prompts_folder(AGENT_FILE);
    let port = InMemoryAgents::default();
    let added = folder.path().to_str().unwrap();
    port.set_command(
        &agent("claude").pane_id,
        &[
            "claude",
            "--model",
            "claude-opus-5-5",
            "--effort",
            "high",
            "--add-dir",
            "/nowhere",
            added,
            "--session-id",
            "s",
            "--",
            "--add-dir /elsewhere",
        ],
    );

    assert_eq!(
        ExploreModels::of_agent(&port, &agent("claude")),
        Some(on_fable(Some(("opus", Effort::High))))
    );
}

#[test]
fn the_worker_is_the_alias_and_effort_the_command_line_names_last() {
    let folder = prompts_folder(AGENT_FILE);
    let added = format!("--add-dir={}", folder.path().display());
    let read = |path: &Path| std::fs::read_to_string(path).ok();
    let models = |argv: &[&str]| {
        let argv: Vec<String> = argv.iter().map(|argument| (*argument).to_owned()).collect();
        ExploreModels::of_command(&argv, read)
    };

    assert_eq!(
        models(&["claude", &added, "--effort", "high"]),
        Some(on_fable(None))
    );
    assert_eq!(
        models(&["claude", &added, "--model", "opus"]),
        Some(on_fable(None))
    );
    assert_eq!(
        models(&["claude", &added, "--model", "opus", "--effort", "extreme"]),
        Some(on_fable(None))
    );
    assert_eq!(
        models(&[
            "claude", &added, "--", "--model", "opus", "--effort", "high"
        ]),
        Some(on_fable(None)),
        "a prompt names them"
    );
    assert_eq!(
        models(&["claude", &added, "--model=sonnet", "--effort=low"]),
        Some(on_fable(Some(("sonnet", Effort::Low))))
    );
    assert_eq!(
        models(&[
            "claude",
            &added,
            "--model",
            "claude-haiku-5-5[1m]",
            "--effort",
            "max"
        ]),
        Some(on_fable(Some(("haiku", Effort::Max))))
    );
    assert_eq!(
        models(&[
            "claude", &added, "--model", "opus", "--effort", "low", "--model", "sonnet"
        ]),
        Some(on_fable(Some(("sonnet", Effort::Low)))),
        "the last of a repeated option, as Claude Code takes it"
    );
    assert_eq!(
        models(&[
            "claude",
            &added,
            "--model",
            "my-gateway-model",
            "--effort",
            "max"
        ]),
        Some(on_fable(None)),
        "no alias for a model of another family"
    );
}

#[test]
fn an_agent_without_the_reviewer_s_settings_runs_the_round_on_its_own_model() {
    let folder = prompts_folder(AGENT_FILE);
    let port = InMemoryAgents::default();
    let added = folder.path().to_str().unwrap();

    port.set_command(&agent("claude").pane_id, &["claude", "--model", "opus"]);
    assert_eq!(
        ExploreModels::of_agent(&port, &agent("claude")),
        None,
        "no --add-dir"
    );

    port.set_command(
        &agent("claude").pane_id,
        &["claude", "--", "--add-dir", added],
    );
    assert_eq!(
        ExploreModels::of_agent(&port, &agent("claude")),
        None,
        "a prompt names it"
    );

    port.set_command(&agent("codex").pane_id, &["codex", "--add-dir", added]);
    assert_eq!(
        ExploreModels::of_agent(&port, &agent("codex")),
        None,
        "Codex"
    );
}

#[test]
fn an_agent_file_without_a_known_effort_or_a_one_word_model_names_no_model() {
    for text in [
        "---\nmodel: claude-fable-5-2\neffort: extreme\n---\n",
        "---\nmodel: claude-fable-5-2\n---\n",
        "---\nmodel: two words\neffort: max\n---\n",
        "model: claude-fable-5-2\neffort: max\n",
        "---\nname: bt-self-review\n---\nmodel: claude-fable-5-2\neffort: max\n",
    ] {
        assert_eq!(ModelEffort::of_agent_file(text), None, "{text}");
    }
}

#[test]
fn the_round_s_line_names_its_model_and_effort_as_the_plugin_s_hooks_module_reads_them() {
    assert_eq!(
        on_fable(None).round_line(),
        "Explore runs on claude-fable-5-2 at max effort."
    );
    let module = include_str!("../../claude-hooks/plugin/hooks/register.ts");
    let pattern = r"/^Explore runs on (\S+) at (low|medium|high|xhigh|max) effort\.$/m";
    assert!(
        module.contains(pattern),
        "the module's pattern changed: {module}"
    );
}
