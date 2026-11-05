use serde_json::{Value, json};

use super::*;

/// A terminal of the space `sp1`, as a snapshot or a `created` event gives it (herdr-core
/// 0.1.17), trimmed, with the OSC 7501 `status` its program reported.
fn terminal(address: &str, status: Value) -> Value {
    let mut terminal = json!({
        "address": address,
        "argv": ["claude"],
        "cols": 80,
        "rows": 24,
        "created_ms": 1,
        "cwd": "/src",
        "labels": {"herdr.space": "sp1", "herdr.tab": "tb1"},
        "screen": "exact",
        "state": "live",
    });
    terminal["status"] = status;
    terminal
}

fn event(kind: &str, terminal: &str, data: Value) -> Event {
    let mut event = json!({"type": kind, "terminal": terminal, "seq": 1});
    event["data"] = data;
    serde_json::from_value(event).unwrap()
}

fn status(state: &str) -> Value {
    json!({"records": {}, "reported": true, "status": {"app": "claude-code", "state": state}})
}

fn roster(snapshot: &[Value]) -> Roster {
    Roster::new(
        snapshot
            .iter()
            .map(|terminal| serde_json::from_value(terminal.clone()).unwrap())
            .collect(),
    )
}

fn arrival(pane: &str) -> HerdrEvent {
    HerdrEvent::AgentDetected {
        pane_id: PaneId(pane.into()),
        workspace_id: WorkspaceId("sp1".into()),
        agent: Some("claude".into()),
        released: false,
        final_status: None,
    }
}

fn departure(pane: &str, last: AgentStatus) -> HerdrEvent {
    HerdrEvent::AgentDetected {
        pane_id: PaneId(pane.into()),
        workspace_id: WorkspaceId("sp1".into()),
        agent: Some("claude".into()),
        released: true,
        final_status: Some(last),
    }
}

#[test]
fn an_agent_arrives_when_a_new_terminals_program_first_reports_a_status() {
    let mut roster = roster(&[]);

    let created = roster
        .observe(&event("created", "t3", terminal("t3", Value::Null)))
        .unwrap();
    let reported = roster
        .observe(&event("status", "t3", status("working")))
        .unwrap();

    assert_eq!((created, reported), (None, Some(arrival("t3"))));
}

#[test]
fn an_agents_turns_are_no_arrival() {
    let mut roster = roster(&[terminal(
        "t3",
        json!({"app": "claude-code", "state": "idle"}),
    )]);

    for state in ["working", "blocked", "idle", "done"] {
        assert_eq!(
            roster
                .observe(&event("status", "t3", status(state)))
                .unwrap(),
            None,
            "{state}"
        );
    }
}

#[test]
fn an_agent_whose_program_exits_leaves_once_with_its_last_status() {
    let mut roster = roster(&[terminal(
        "t3",
        json!({"app": "claude-code", "state": "done"}),
    )]);

    // herdr-core clears the status as the program exits, then tells the terminal finished.
    let cleared = roster
        .observe(&event(
            "status",
            "t3",
            json!({"records": {}, "reported": true, "status": null}),
        ))
        .unwrap();
    let finished = roster
        .observe(&event(
            "finished",
            "t3",
            json!({"exit_code": 0, "reason": "exited"}),
        ))
        .unwrap();

    assert_eq!(
        (cleared, finished),
        (Some(departure("t3", AgentStatus::Done)), None)
    );
}

#[test]
fn an_agent_whose_terminal_is_lost_leaves() {
    let mut roster = roster(&[terminal(
        "t3",
        json!({"app": "claude-code", "state": "working"}),
    )]);

    assert_eq!(
        roster.observe(&event("lost", "t3", json!({}))).unwrap(),
        Some(departure("t3", AgentStatus::Working))
    );
}

#[test]
fn a_program_outside_the_spaces_of_herdr_app_is_no_agent_of_the_reviewers() {
    let mut outside = terminal("t4", Value::Null);
    outside["labels"] = json!({});
    let mut roster = roster(&[outside]);

    assert_eq!(
        roster
            .observe(&event("status", "t4", status("idle")))
            .unwrap(),
        None
    );
}

#[test]
fn an_agent_arrives_in_a_terminal_that_joins_a_space_later() {
    let mut started_by_hand = terminal("t4", Value::Null);
    started_by_hand["labels"] = json!({});
    let mut roster = roster(&[started_by_hand]);

    let labelled = roster
        .observe(&event(
            "labels",
            "t4",
            json!({"labels": {"herdr.space": "sp1", "herdr.tab": "tb1"}}),
        ))
        .unwrap();
    let reported = roster
        .observe(&event("status", "t4", status("idle")))
        .unwrap();

    assert_eq!((labelled, reported), (None, Some(arrival("t4"))));
}

#[test]
fn an_agent_that_enters_a_subscription_by_its_labels_arrives() {
    let mut roster = roster(&[]);

    let entered = roster
        .observe(&event(
            "labels",
            "t5",
            json!({
                "labels": {"herdr.space": "sp1", "herdr.tab": "tb1"},
                "record": terminal("t5", json!({"app": "claude-code", "state": "idle"})),
            }),
        ))
        .unwrap();

    assert_eq!(entered, Some(arrival("t5")));
}

#[test]
fn another_agent_in_the_same_terminal_arrives() {
    let mut roster = roster(&[terminal("t3", json!({"app": "codex", "state": "idle"}))]);

    assert_eq!(
        roster
            .observe(&event("status", "t3", status("idle")))
            .unwrap(),
        Some(arrival("t3"))
    );
}

#[test]
fn an_agent_arrives_once_its_terminal_joins_a_space_and_leaves_with_it() {
    let mut started_by_hand = terminal("t4", json!({"app": "claude-code", "state": "idle"}));
    started_by_hand["labels"] = json!({});
    let mut roster = roster(&[started_by_hand]);

    let adopted = roster
        .observe(&event(
            "labels",
            "t4",
            json!({"labels": {"herdr.space": "sp1", "herdr.tab": "tb1"}}),
        ))
        .unwrap();
    let parked = roster
        .observe(&event("labels", "t4", json!({"labels": {}})))
        .unwrap();

    assert_eq!(
        (adopted, parked),
        (
            Some(arrival("t4")),
            Some(departure("t4", AgentStatus::Idle))
        )
    );
}

#[test]
fn an_agent_arrives_again_in_a_lost_terminal_that_restarted() {
    let mut roster = roster(&[terminal(
        "t3",
        json!({"app": "claude-code", "state": "idle"}),
    )]);
    roster.observe(&event("lost", "t3", json!({}))).unwrap();

    // herdr-core gives a kept terminal a shell, whose agent reports again.
    let restarted = roster
        .observe(&event("restarted", "t3", terminal("t3", Value::Null)))
        .unwrap();
    let reported = roster
        .observe(&event("status", "t3", status("idle")))
        .unwrap();

    assert_eq!((restarted, reported), (None, Some(arrival("t3"))));
}

#[test]
fn a_documents_change_names_no_terminal_and_tells_nothing() {
    let mut roster = roster(&[]);
    let layout: Event = serde_json::from_value(
        json!({"type": "doc", "seq": 10, "data": {"name": "herdr-app", "version": 1}}),
    )
    .unwrap();

    assert_eq!(roster.observe(&layout).unwrap(), None);
}

#[test]
fn an_agent_moved_to_another_tab_arrives_there() {
    let mut roster = roster(&[terminal(
        "t3",
        json!({"app": "claude-code", "state": "idle"}),
    )]);

    let moved = roster
        .observe(&event(
            "labels",
            "t3",
            json!({"labels": {"herdr.space": "sp1", "herdr.tab": "tb2"}}),
        ))
        .unwrap();
    let relabelled = roster
        .observe(&event(
            "labels",
            "t3",
            json!({"labels": {"herdr.space": "sp1", "herdr.tab": "tb2", "note": "x"}}),
        ))
        .unwrap();

    assert_eq!((moved, relabelled), (Some(arrival("t3")), None));
}

#[test]
fn a_terminal_in_a_space_but_no_tab_holds_no_agent_yet() {
    let mut half_labelled = terminal("t4", json!({"app": "claude-code", "state": "idle"}));
    half_labelled["labels"] = json!({"herdr.space": "sp1"});
    let mut roster = roster(&[]);

    let created = roster
        .observe(&event("created", "t4", half_labelled))
        .unwrap();
    let tabbed = roster
        .observe(&event(
            "labels",
            "t4",
            json!({"labels": {"herdr.space": "sp1", "herdr.tab": "tb1"}}),
        ))
        .unwrap();

    assert_eq!((created, tabbed), (None, Some(arrival("t4"))));
}

/// A `labels` event of `t3` in the tab `tb1`, with the session label `session` if any.
fn session_labels(session: Option<&str>) -> Event {
    let mut labels = json!({"herdr.space": "sp1", "herdr.tab": "tb1"});
    if let Some(session) = session {
        labels["herdr-reviewer.session"] = session.into();
    }
    event("labels", "t3", json!({"labels": labels}))
}

#[test]
fn the_session_label_of_an_agent_tells_each_change_of_its_session() {
    let mut roster = roster(&[terminal(
        "t3",
        json!({"app": "claude-code", "state": "idle"}),
    )]);
    let changed = || Some(HerdrEvent::AgentSessionChanged(PaneId("t3".into())));

    let reported = roster.observe(&session_labels(Some("claude:s1"))).unwrap();
    let same = roster.observe(&session_labels(Some("claude:s1"))).unwrap();
    let turn = roster
        .observe(&event("status", "t3", status("working")))
        .unwrap();
    let replaced = roster.observe(&session_labels(Some("claude:s2"))).unwrap();
    let forgotten = roster.observe(&session_labels(None)).unwrap();

    assert_eq!(
        [reported, same, turn, replaced, forgotten],
        [changed(), None, None, changed(), changed()]
    );
}

#[test]
fn the_session_label_of_a_terminal_without_an_agent_tells_nothing() {
    let mut roster = roster(&[terminal("t3", Value::Null)]);

    assert_eq!(
        roster.observe(&session_labels(Some("claude:s1"))).unwrap(),
        None
    );
}

/// The data of a `foreground` event: the new leader of the terminal's foreground process group,
/// as herdr-core 0.1.18 sends it.
fn leader(name: &str) -> Value {
    json!({"argv": [name], "cwd": "/src", "name": name, "pid": 9})
}

/// A `labels` event of `t3` in the tab `tb1`, its session label naming Claude Code's `session`.
fn claude_session(session: &str) -> Event {
    session_labels(Some(&format!("claude:{session}")))
}

#[test]
fn claude_code_that_stops_reporting_stays_while_it_holds_the_foreground_on_its_session() {
    let mut reporting = terminal("t3", json!({"app": "claude-code", "state": "done"}));
    reporting["foreground"] = leader("claude");
    reporting["labels"]["herdr-reviewer.session"] = json!("claude:s1");
    let mut roster = roster(&[reporting]);

    let cleared = roster
        .observe(&event(
            "status",
            "t3",
            json!({"records": {}, "reported": true, "status": null}),
        ))
        .unwrap();
    let suspended = roster
        .observe(&event("foreground", "t3", leader("zsh")))
        .unwrap();
    let resumed = roster
        .observe(&event("foreground", "t3", leader("claude")))
        .unwrap();

    assert_eq!(
        (cleared, suspended, resumed),
        (
            None,
            Some(departure("t3", AgentStatus::Unknown)),
            Some(arrival("t3"))
        )
    );
}

#[test]
fn claude_code_arrives_as_its_session_starts_not_as_it_takes_the_foreground() {
    let mut shell = terminal("t3", Value::Null);
    shell["foreground"] = leader("zsh");
    let mut roster = roster(&[shell]);

    let started = roster
        .observe(&event("foreground", "t3", leader("claude")))
        .unwrap();
    let session = roster.observe(&claude_session("s1")).unwrap();
    let reported = roster
        .observe(&event("status", "t3", status("idle")))
        .unwrap();

    assert_eq!(
        (started, session, reported),
        (None, Some(arrival("t3")), None)
    );
}
