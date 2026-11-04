use super::*;

fn arguments(text: &str) -> Vec<String> {
    text.split_whitespace().map(str::to_owned).collect()
}

#[test]
fn a_fork_keeps_the_options_that_shape_the_agent_and_drops_those_that_pick_a_session() {
    let kept = kept(&arguments(
        "--model claude-sonnet-5-5 --resume abc --strict-mcp-config --mcp-config a.json b.json \
         --permission-mode=dontAsk -c --session-id s1 --verbose --allowedTools Read Grep \
         --add-dir /tmp",
    ))
    .unwrap();

    assert_eq!(
        kept.arguments,
        arguments(
            "--strict-mcp-config --mcp-config a.json b.json --permission-mode=dontAsk \
             --allowedTools Read Grep --add-dir /tmp"
        )
    );
    assert_eq!(kept.model.as_deref(), Some("claude-sonnet-5-5"));
}

#[test]
fn a_prompt_on_the_command_line_does_not_reach_a_fork() {
    let kept = kept(&arguments(
        "fix the bug --model=opus --resume --effort high",
    ))
    .unwrap();

    assert_eq!(kept.arguments, arguments("--effort high"));
    assert_eq!(kept.model.as_deref(), Some("opus"));
}

#[test]
fn an_agent_with_an_unknown_option_is_not_forked() {
    assert!(kept(&arguments("--some-new-option value")).is_err());
}

#[test]
fn the_agent_s_settings_come_apart_from_the_arguments_a_fork_keeps() {
    // As herdr-bowtie starts its agents.
    let kept = kept(&arguments(
        "--resume 9ff3 --model opus --effort high --settings /share/claude-settings.json \
         --mcp-config /share/mcp.json",
    ))
    .unwrap();

    assert_eq!(
        kept.arguments,
        arguments("--effort high --mcp-config /share/mcp.json")
    );
    assert_eq!(
        kept.settings.as_deref(),
        Some("/share/claude-settings.json")
    );
    assert_eq!(kept.model.as_deref(), Some("opus"));
}

#[test]
fn what_follows_a_double_dash_is_the_prompt_even_when_it_looks_like_an_option() {
    // As herdr-bowtie starts a worker with its kickoff.
    let kept = kept(&arguments(
        "--model opus --settings /share/s.json -n run-ahead -- herdr-bowtie kickoff --resume x",
    ))
    .unwrap();

    assert!(kept.arguments.is_empty(), "{:?}", kept.arguments);
    assert_eq!(kept.settings.as_deref(), Some("/share/s.json"));
    assert_eq!(kept.model.as_deref(), Some("opus"));
}

#[test]
fn inline_settings_come_apart_too() {
    let kept = kept(&[r#"--settings={"env":{"A":"1"}}"#.to_owned()]).unwrap();

    assert!(kept.arguments.is_empty());
    assert_eq!(kept.settings.as_deref(), Some(r#"{"env":{"A":"1"}}"#));
}
