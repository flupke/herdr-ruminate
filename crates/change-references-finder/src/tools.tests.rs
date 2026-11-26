use super::*;

#[test]
fn reads_the_newest_pull_request_of_the_repository_s_own_branch_and_its_body() {
    let output = r#"[{"body":"","isCrossRepository":true,"number":13,"title":"Fork","url":"https://github.com/acme/shop/pull/13"},{"body":"Fixes #7","isCrossRepository":false,"number":12,"title":"Name past rounds","url":"https://github.com/acme/shop/pull/12"}]"#;

    assert_eq!(
        pull_request_of(output, None),
        Some(FoundPullRequest {
            pull_request: PullRequest {
                number: 12,
                title: "Name past rounds".to_owned(),
                url: "https://github.com/acme/shop/pull/12".to_owned(),
            },
            body: "Fixes #7".to_owned(),
        })
    );
}

#[test]
fn an_empty_list_has_no_pull_request() {
    assert_eq!(pull_request_of("[]", None), None);
}

#[test]
fn a_command_that_cannot_start_or_fails_gives_nothing() {
    assert_eq!(run(Command::new("/nonexistent/gh"), TIMEOUT), None);
    let mut failing = Command::new("sh");
    failing.args(["-c", "echo partial; exit 1"]);
    assert_eq!(run(failing, TIMEOUT), None);
    let mut succeeding = Command::new("sh");
    succeeding.args(["-c", "echo found"]);
    assert_eq!(run(succeeding, TIMEOUT).as_deref(), Some("found\n"));
}

#[test]
fn a_command_past_its_time_is_killed_and_gives_nothing() {
    let mut hanging = Command::new("sh");
    hanging.args(["-c", "exec sleep 600"]);

    assert_eq!(run(hanging, Duration::ZERO), None);
}

#[test]
fn takes_the_pull_request_of_the_fork_the_change_is_pushed_to_and_no_other() {
    let output = r#"[{"body":"","isCrossRepository":true,"headRepositoryOwner":{"login":"stranger"},"number":13,"title":"Other","url":"https://github.com/acme/shop/pull/13"},{"body":"","isCrossRepository":true,"headRepositoryOwner":{"login":"Me"},"number":12,"title":"Mine","url":"https://github.com/acme/shop/pull/12"}]"#;

    assert_eq!(
        pull_request_of(output, Some("me")).map(|found| found.pull_request.number),
        Some(12)
    );
    assert_eq!(pull_request_of(output, None), None);
}
