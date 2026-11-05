//! The hc backend against a real herdr-core, on a private core of its own
//! ([`hc_test_support::PrivateCore`]), never the user's core. Opt-in, since each test starts a
//! core of its own: `cargo nextest run -p hc-client --run-ignored only`, with the dev shell's
//! pinned `hc` (`TEST_HC_BIN_PATH`), or else `hc` on the `PATH`.

use std::collections::BTreeMap;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use hc_client::HcClient;
use hc_test_support::PrivateCore;
use herdr_client::client::EventCanceller;
use herdr_client::protocol::{AgentHost, AgentPort, AgentStatus, HerdrEvent, PaneId, TabId};

/// How fast the person types.
const PACE: Duration = Duration::from_millis(300);

/// A shell line that reports `state` as Claude Code does, with OSC 7501.
fn report(state: &str) -> String {
    format!(r"printf '\033]7501;app=claude-code:state={state}\007';")
}

/// Waits until the agent of `pane` reports `status`, and returns the statuses until then.
fn wait_for(client: &HcClient, pane: &PaneId, status: AgentStatus) -> Vec<AgentStatus> {
    let mut seen = Vec::new();
    client
        .forward_agent_status(pane, &EventCanceller::default(), &mut |reported| {
            seen.push(reported);
            reported != status
        })
        .unwrap();
    seen
}

#[test]
#[ignore = "needs herdr-core: hc on the PATH or TEST_HC_BIN_PATH"]
fn the_agents_of_a_tab_are_its_terminals_that_report_a_status() {
    let core = PrivateCore::start();
    let folder = tempfile::tempdir().unwrap();
    let client = core.client();
    let agent = core.run(folder.path(), &format!("{} read -r line", report("idle")));
    let shell = core.run(folder.path(), "exec sleep 600");
    wait_for(&client, &agent, AgentStatus::Idle);

    let agents = client.list_agents().unwrap();

    let [listed] = agents.as_slice() else {
        panic!("one agent: {agents:?}");
    };
    assert_eq!(listed.pane_id, agent);
    assert_eq!(listed.tab_id, TabId("tb1".into()));
    assert_eq!(listed.agent.as_deref(), Some("claude"));
    assert_eq!(listed.agent_status, AgentStatus::Idle);
    assert_eq!(client.get_agent(&shell).unwrap(), None);
    assert_eq!(client.get_agent(&PaneId("t999".into())).unwrap(), None);
    // herdr-core names the foreground process a moment after the program starts.
    core.wait_for_foreground(&agent);
    let processes = client.pane_process_info(&agent).unwrap();
    assert_eq!(
        processes
            .foreground_processes
            .iter()
            .map(|process| (Some(process.pid), process.name.as_str()))
            .collect::<Vec<_>>(),
        [(processes.foreground_process_group_id, "bash")]
    );
}

#[test]
#[ignore = "needs herdr-core: hc on the PATH or TEST_HC_BIN_PATH"]
fn a_prompt_reaches_the_agent_as_typed_and_the_agent_starts_on_it() {
    let core = PrivateCore::start();
    let folder = tempfile::tempdir().unwrap();
    let client = core.client();
    let agent = core.run(
        folder.path(),
        &format!(
            "{} read -r line; printf '%s' \"$line\" > prompt; {} exec sleep 600",
            report("idle"),
            report("working")
        ),
    );
    wait_for(&client, &agent, AgentStatus::Idle);

    client
        .prompt_agent(&agent, "Review crates/hc-client")
        .unwrap();

    // The agent wrote the prompt before it went working, which the send waited for.
    assert_eq!(
        std::fs::read_to_string(folder.path().join("prompt")).unwrap(),
        "Review crates/hc-client"
    );
}

#[test]
#[ignore = "needs herdr-core: hc on the PATH or TEST_HC_BIN_PATH"]
fn an_agent_that_never_starts_on_a_prompt_is_told() {
    let core = PrivateCore::start();
    let folder = tempfile::tempdir().unwrap();
    let client = core.client();
    let agent = core.run(folder.path(), &format!("{} exec cat", report("idle")));
    wait_for(&client, &agent, AgentStatus::Idle);

    // herdr-core's own window of a few seconds after the send tells that `cat` never worked.
    assert!(matches!(
        client.prompt_agent(&agent, "Review crates/hc-client"),
        Err(herdr_client::Error::AgentNotStarted { .. })
    ));
}

#[test]
#[ignore = "needs herdr-core: hc on the PATH or TEST_HC_BIN_PATH"]
fn a_prompt_to_an_agent_that_waits_on_its_user_types_nothing() {
    let core = PrivateCore::start();
    let folder = tempfile::tempdir().unwrap();
    let client = core.client();
    let agent = core.run(
        folder.path(),
        &format!(
            "{} read -r line; printf '%s' \"$line\" > typed; {} read -r line",
            report("blocked"),
            report("done")
        ),
    );
    wait_for(&client, &agent, AgentStatus::Blocked);

    let refused = client.submit_prompt(&agent, "Review crates/hc-client");

    assert!(
        matches!(&refused, Err(herdr_client::Error::NotTyped { reason }) if reason.contains("waits on its user")),
        "{refused:?}"
    );
    // A key pressed after the refusal is the first the agent reads.
    core.hc(&["keys", &agent.0, "m", "enter"]);
    wait_for(&client, &agent, AgentStatus::Done);
    assert_eq!(
        std::fs::read_to_string(folder.path().join("typed")).unwrap(),
        "m"
    );
}

#[test]
#[ignore = "needs herdr-core: hc on the PATH or TEST_HC_BIN_PATH"]
fn a_prompt_to_an_agent_that_blocked_while_the_person_typed_types_nothing() {
    let core = PrivateCore::start();
    let folder = tempfile::tempdir().unwrap();
    let client = core.client();
    // The agent works until the person types `!`, then waits on them.
    let agent = core.run(
        folder.path(),
        &format!(
            "{} while read -rn1 key && [ \"$key\" != '!' ]; do :; done; {} read -r line; \
             printf '%s' \"$line\" > typed; {} read -r line",
            report("working"),
            report("blocked"),
            report("done")
        ),
    );
    wait_for(&client, &agent, AgentStatus::Working);
    let mut person = core.person_at(&agent);
    person.types("ab", PACE);
    let sender = {
        let (client, agent) = (client.clone(), agent.clone());
        thread::spawn(move || {
            let refused = client.submit_prompt(&agent, "Review crates/hc-client");
            (refused, Instant::now())
        })
    };

    person.types("!cd", PACE);
    let typed = Instant::now();
    let (refused, at) = sender.join().unwrap();

    assert!(
        matches!(&refused, Err(herdr_client::Error::NotTyped { reason }) if reason.contains("waits on its user")),
        "{refused:?}"
    );
    // A refusal before the send waited for the person to stop would have come at once.
    assert!(
        at >= typed + Duration::from_secs(2),
        "herdr-core refused before it waited"
    );
    core.hc(&["keys", &agent.0, "m", "enter"]);
    wait_for(&client, &agent, AgentStatus::Done);
    assert_eq!(
        std::fs::read_to_string(folder.path().join("typed")).unwrap(),
        "cdm"
    );
}

#[test]
#[ignore = "needs herdr-core: hc on the PATH or TEST_HC_BIN_PATH"]
fn a_prompt_waits_until_the_person_typing_in_the_agent_s_terminal_stops() {
    let core = PrivateCore::start();
    let folder = tempfile::tempdir().unwrap();
    let client = core.client();
    let agent = core.run(
        folder.path(),
        &format!(
            "{} for n in 1 2; do read -r line; printf '%s\\n' \"$line\" >> lines; done; {} exec \
             sleep 600",
            report("idle"),
            report("working")
        ),
    );
    wait_for(&client, &agent, AgentStatus::Idle);
    let mut person = core.person_at(&agent);
    person.types("my", PACE);
    let sender = {
        let (client, agent) = (client.clone(), agent.clone());
        thread::spawn(move || client.prompt_agent(&agent, "Review crates/hc-client"))
    };

    person.types(" own words", PACE);
    person.presses_enter();
    sender.join().unwrap().unwrap();

    // herdr-core typed nothing among the person's keys: their line went first, whole.
    assert_eq!(
        std::fs::read_to_string(folder.path().join("lines")).unwrap(),
        "my own words\nReview crates/hc-client\n"
    );
}

#[test]
#[ignore = "needs herdr-core: hc on the PATH or TEST_HC_BIN_PATH"]
fn an_agents_statuses_follow_its_turns_and_end_with_its_program() {
    let core = PrivateCore::start();
    let folder = tempfile::tempdir().unwrap();
    let client = core.client();
    let agent = core.run(
        folder.path(),
        &format!(
            "{} read -r line; {} {} read -r line",
            report("idle"),
            report("working"),
            report("done")
        ),
    );
    wait_for(&client, &agent, AgentStatus::Idle);
    let (statuses, seen) = mpsc::channel();
    let watcher = {
        let (client, agent) = (client.clone(), agent.clone());
        thread::spawn(move || {
            client
                .forward_agent_status(&agent, &EventCanceller::default(), &mut |status| {
                    statuses.send(status).is_ok()
                })
                .unwrap();
        })
    };
    assert_eq!(seen.recv().unwrap(), AgentStatus::Idle);

    client.submit_prompt(&agent, "go").unwrap();
    assert_eq!(seen.recv().unwrap(), AgentStatus::Working);
    assert_eq!(seen.recv().unwrap(), AgentStatus::Done);
    client.submit_prompt(&agent, "exit").unwrap();

    // The program exits: its status clears, and the stream ends.
    watcher.join().unwrap();
    assert_eq!(seen.try_iter().collect::<Vec<_>>(), [AgentStatus::Unknown]);
}

#[test]
#[ignore = "needs herdr-core: hc on the PATH or TEST_HC_BIN_PATH"]
fn an_agent_that_exits_leaves_and_labels_reach_its_terminal() {
    let core = PrivateCore::start();
    let folder = tempfile::tempdir().unwrap();
    let client = core.client();
    let agent = core.run(folder.path(), &format!("{} read -r line", report("idle")));
    wait_for(&client, &agent, AgentStatus::Idle);
    let (events, told) = mpsc::channel();
    {
        let client = client.clone();
        thread::spawn(move || {
            client.forward_events(&EventCanceller::default(), &mut |event| {
                events.send(event).is_ok()
            })
        });
    }

    client
        .set_labels(
            &agent,
            &BTreeMap::from([("review.mark".to_owned(), "1".to_owned())]),
        )
        .unwrap();
    assert_eq!(
        client
            .labels(&agent)
            .unwrap()
            .get("review.mark")
            .map(String::as_str),
        Some("1")
    );
    assert_eq!(
        client.place(&agent).unwrap(),
        Some((
            herdr_client::protocol::WorkspaceId("sp1".into()),
            herdr_client::protocol::TabId("tb1".into())
        ))
    );
    // The stream starts with the agents it holds: the subscription holds.
    assert_eq!(
        told.recv().unwrap(),
        HerdrEvent::AgentSessionChanged(agent.clone())
    );
    client.submit_prompt(&agent, "exit").unwrap();

    assert_eq!(
        told.recv().unwrap(),
        HerdrEvent::AgentDetected {
            pane_id: agent,
            workspace_id: herdr_client::protocol::WorkspaceId("sp1".into()),
            agent: Some("claude".into()),
            released: true,
            final_status: Some(AgentStatus::Idle),
        }
    );
}

#[test]
#[ignore = "needs herdr-core: hc on the PATH or TEST_HC_BIN_PATH"]
fn the_session_an_agents_hook_names_in_its_label_is_the_agents_and_each_change_is_told() {
    let core = PrivateCore::start();
    let folder = tempfile::tempdir().unwrap();
    let client = core.client();
    let agent = core.run(folder.path(), &format!("{} read -r line", report("idle")));
    wait_for(&client, &agent, AgentStatus::Idle);
    let (events, told) = mpsc::channel();
    {
        let client = client.clone();
        thread::spawn(move || {
            client.forward_events(&EventCanceller::default(), &mut |event| {
                events.send(event).is_ok()
            })
        });
    }
    // The stream starts with the agents it holds: the subscription holds.
    assert_eq!(
        told.recv().unwrap(),
        HerdrEvent::AgentSessionChanged(agent.clone())
    );

    client.report_session(&agent, "claude", "s1").unwrap();
    assert_eq!(
        told.recv().unwrap(),
        HerdrEvent::AgentSessionChanged(agent.clone())
    );
    let session = |client: &HcClient| {
        client
            .get_agent(&agent)
            .unwrap()
            .and_then(|agent| agent.agent_session)
            .map(|session| session.value)
    };
    assert_eq!(session(&client).as_deref(), Some("s1"));

    // A later session's label stays when an earlier one ends; the session that ends removes it.
    client.report_session(&agent, "claude", "s2").unwrap();
    assert_eq!(
        told.recv().unwrap(),
        HerdrEvent::AgentSessionChanged(agent.clone())
    );
    client.forget_session(&agent, "claude", "s1").unwrap();
    assert_eq!(session(&client).as_deref(), Some("s2"));
    client.forget_session(&agent, "claude", "s2").unwrap();
    assert_eq!(
        told.recv().unwrap(),
        HerdrEvent::AgentSessionChanged(agent.clone())
    );
    assert_eq!(session(&client), None);
}

/// The variable that makes [`send_from_this_terminal`] send, to the terminal it names.
const SEND_TO: &str = "PRIVATE_CORE_SEND_TO";

#[test]
#[ignore = "needs herdr-core: hc on the PATH or TEST_HC_BIN_PATH"]
fn a_prompt_sent_from_a_terminal_of_the_core_reaches_the_agent_untagged() {
    let core = PrivateCore::start();
    let folder = tempfile::tempdir().unwrap();
    let client = core.client();
    let agent = core.run(
        folder.path(),
        &format!(
            "{} read -r line; printf '%s' \"$line\" > typed; {} read -r line",
            report("idle"),
            report("done")
        ),
    );
    wait_for(&client, &agent, AgentStatus::Idle);

    // The review pane sends from a terminal of the core, which herdr-core could name as the
    // caller, and tag what it sends.
    let this_test = std::env::current_exe().unwrap();
    let sender = core.start_in(
        folder.path(),
        &[
            "env",
            &format!("{SEND_TO}={}", agent.0),
            this_test.to_str().unwrap(),
            "send_from_this_terminal",
            "--exact",
            "--ignored",
        ],
    );

    // The sender's stream ends with its program.
    client
        .forward_agent_status(&sender, &EventCanceller::default(), &mut |_| true)
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&core.hc(&["get", &sender.0])).unwrap()["exit_code"],
        0,
        "the sender failed: {}",
        core.screen(&sender)
    );
    wait_for(&client, &agent, AgentStatus::Done);
    assert_eq!(
        std::fs::read_to_string(folder.path().join("typed")).unwrap(),
        "Review crates/hc-client"
    );
}

/// What [`a_prompt_sent_from_a_terminal_of_the_core_reaches_the_agent_untagged`] runs in a
/// terminal of the core: it sends a prompt there as the review pane does. Without its variable,
/// it does nothing.
#[test]
#[ignore = "run by a_prompt_sent_from_a_terminal_of_the_core_reaches_the_agent_untagged"]
fn send_from_this_terminal() {
    let Some(agent) = std::env::var_os(SEND_TO) else {
        return;
    };
    HcClient::from_env()
        .unwrap()
        .submit_prompt(
            &PaneId(agent.into_string().unwrap()),
            "Review crates/hc-client",
        )
        .unwrap();
}
