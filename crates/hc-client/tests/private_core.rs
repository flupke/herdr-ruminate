use std::collections::BTreeMap;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use hc_client::HcClient;
use hc_test_support::{
    PrivateCore, input_line, line_agent, line_agent_keeping_its_line, line_agent_without_turns,
    report, wait_until_read, written,
};
use herdr_client::client::EventCanceller;
use herdr_client::protocol::{
    AgentHost, AgentPort, AgentStatus, HerdrEvent, PaneId, PromptWait, TabId,
};

const PACE: Duration = Duration::from_millis(300);

type Telling = thread::JoinHandle<(herdr_client::Result<()>, Vec<Option<PromptWait>>)>;

fn send_telling_waits(
    client: &HcClient,
    agent: &PaneId,
    text: &'static str,
) -> (Telling, mpsc::Receiver<Option<PromptWait>>) {
    send_withdrawn_by(client, agent, text, EventCanceller::default())
}

fn send_withdrawn_by(
    client: &HcClient,
    agent: &PaneId,
    text: &'static str,
    withdrawal: EventCanceller,
) -> (Telling, mpsc::Receiver<Option<PromptWait>>) {
    let (client, agent) = (client.clone(), agent.clone());
    let (told, waits) = mpsc::channel();
    let sender = thread::spawn(move || {
        let mut all = Vec::new();
        let sent = client.submit_prompt_telling_waits(
            &agent,
            text,
            &mut |wait| {
                all.push(wait.clone());
                let _ = told.send(wait);
            },
            &withdrawal,
        );
        (sent, all)
    });
    (sender, waits)
}

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
    let agent = core.run(
        folder.path(),
        &format!("{} {} read -r line", input_line(), report("idle")),
    );
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
            "{} {} read -r line; printf '%s' \"$line\" > prompt; {} exec sleep 600",
            input_line(),
            report("idle"),
            report("working")
        ),
    );
    wait_for(&client, &agent, AgentStatus::Idle);

    client
        .prompt_agent(&agent, "Review crates/hc-client")
        .unwrap();

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
    let agent = core.run(
        folder.path(),
        &format!("{} {} exec cat", input_line(), report("idle")),
    );
    wait_for(&client, &agent, AgentStatus::Idle);

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
            "{} {} read -r line; printf '%s' \"$line\" > typed; {} read -r line",
            input_line(),
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
    let core = PrivateCore::holding_unsent_typing_for(Duration::ZERO);
    let folder = tempfile::tempdir().unwrap();
    let client = core.client();
    let agent = core.run(
        folder.path(),
        &format!(
            "{} {} while read -rn1 key && [ \"$key\" != '!' ]; do :; done; {} read -r line; \
             printf '%s' \"$line\" > typed; {} read -r line",
            input_line(),
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
            "{} {} for n in 1 2; do read -r line; printf '%s\\n' \"$line\" >> lines; done; {} \
             exec sleep 600",
            input_line(),
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
            "{} {} read -r line; {} {} read -r line",
            input_line(),
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

    watcher.join().unwrap();
    assert_eq!(seen.try_iter().collect::<Vec<_>>(), [AgentStatus::Unknown]);
}

#[test]
#[ignore = "needs herdr-core: hc on the PATH or TEST_HC_BIN_PATH"]
fn an_agent_that_exits_leaves_and_labels_reach_its_terminal() {
    let core = PrivateCore::start();
    let folder = tempfile::tempdir().unwrap();
    let client = core.client();
    let agent = core.run(
        folder.path(),
        &format!("{} {} read -r line", input_line(), report("idle")),
    );
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
    let agent = core.run(
        folder.path(),
        &format!("{} {} read -r line", input_line(), report("idle")),
    );
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

const PROMPT: &str = "Review crates/hc-client";

#[test]
#[ignore = "needs herdr-core: hc on the PATH or TEST_HC_BIN_PATH"]
fn an_empty_input_line_gets_the_prompt_at_once() {
    let core = PrivateCore::holding_unsent_typing_for(Duration::ZERO);
    let folder = tempfile::tempdir().unwrap();
    let client = core.client();
    let agent = core.run(folder.path(), &line_agent(""));
    wait_for(&client, &agent, AgentStatus::Idle);
    // herdr-core types in a terminal that just started only once its output settled, a second
    // later (hc 0.1.21).
    thread::sleep(Duration::from_secs(3));

    let start = std::time::SystemTime::now();
    let (sender, _) = send_telling_waits(&client, &agent, PROMPT);
    let (sent, waits) = sender.join().unwrap();

    sent.unwrap();
    let submitted = written(folder.path(), "submitted");
    let submitted = Duration::from_nanos(submitted.trim().parse().unwrap());
    let took = submitted - start.duration_since(std::time::UNIX_EPOCH).unwrap();
    assert!(took < Duration::from_secs(1), "the prompt took {took:?}");
    eprintln!("the prompt reached the agent after {took:?}");
    assert_eq!(written(folder.path(), "lines"), format!("{PROMPT}\n"));
    assert_eq!(waits, []);
}

#[test]
#[ignore = "needs herdr-core: hc on the PATH or TEST_HC_BIN_PATH"]
fn a_line_with_text_holds_the_prompt_until_it_empties_however_long() {
    let core = PrivateCore::holding_unsent_typing_for(Duration::ZERO);
    let folder = tempfile::tempdir().unwrap();
    let client = core.client();
    let agent = core.run(folder.path(), &line_agent(""));
    wait_for(&client, &agent, AgentStatus::Idle);
    let mut person = core.person_at(&agent);
    person.types("my own", Duration::from_millis(50));
    wait_until_read(folder.path(), 'n');

    let (sender, waits) = send_telling_waits(&client, &agent, PROMPT);
    assert_eq!(waits.recv().unwrap(), Some(PromptWait::TypedText));
    // Well past herdr-core's quiet pause after the person's last key, which alone holds a send.
    thread::sleep(Duration::from_secs(8));
    assert_eq!(written(folder.path(), "lines"), "");
    person.presses_enter();
    let (sent, told) = sender.join().unwrap();

    sent.unwrap();
    assert_eq!(
        written(folder.path(), "lines"),
        format!("my own\n{PROMPT}\n")
    );
    assert_eq!(told, [Some(PromptWait::TypedText), None]);
    assert!(
        !written(folder.path(), "keys").contains('\u{15}'),
        "a Ctrl+U reached the agent"
    );
}

#[test]
#[ignore = "needs herdr-core: hc on the PATH or TEST_HC_BIN_PATH"]
fn a_prompt_withdrawn_while_text_in_the_line_holds_it_stops_waiting_and_is_never_typed() {
    let core = PrivateCore::holding_unsent_typing_for(Duration::ZERO);
    let folder = tempfile::tempdir().unwrap();
    let client = core.client();
    let agent = core.run(folder.path(), &line_agent(""));
    wait_for(&client, &agent, AgentStatus::Idle);
    let mut person = core.person_at(&agent);
    person.types("my own", Duration::from_millis(50));
    wait_until_read(folder.path(), 'n');
    let withdrawal = EventCanceller::default();

    let (sender, waits) = send_withdrawn_by(&client, &agent, PROMPT, withdrawal.clone());
    assert_eq!(waits.recv().unwrap(), Some(PromptWait::TypedText));
    let withdrawn = Instant::now();
    withdrawal.cancel();
    let (sent, told) = sender.join().unwrap();

    assert!(
        withdrawn.elapsed() < Duration::from_secs(1),
        "the send ended {:?} after its withdrawal",
        withdrawn.elapsed()
    );
    assert!(
        matches!(sent, Err(herdr_client::Error::Cancelled)),
        "{sent:?}"
    );
    assert_eq!(told, [Some(PromptWait::TypedText)]);
    person.presses_enter();
    // Past herdr-core's quiet pause after the person's Enter.
    thread::sleep(Duration::from_secs(5));
    assert_eq!(written(folder.path(), "lines"), "my own\n");
}

#[test]
#[ignore = "needs herdr-core: hc on the PATH or TEST_HC_BIN_PATH"]
fn a_prompt_withdrawn_while_herdr_core_waits_out_its_pause_is_never_typed() {
    let core = PrivateCore::holding_unsent_typing_for(Duration::ZERO);
    let folder = tempfile::tempdir().unwrap();
    let client = core.client();
    let agent = core.run(folder.path(), &line_agent(""));
    wait_for(&client, &agent, AgentStatus::Idle);
    thread::sleep(Duration::from_secs(3));
    let mut person = core.person_at(&agent);
    // herdr-core waits out its quiet pause after a key, even one erased, before it types a send.
    person.types("x", Duration::ZERO);
    person.presses_backspace();
    wait_until_read(folder.path(), '\u{7f}');
    let withdrawal = EventCanceller::default();

    let (sender, _) = send_withdrawn_by(&client, &agent, PROMPT, withdrawal.clone());
    thread::sleep(Duration::from_millis(500));
    withdrawal.cancel();
    let (sent, told) = sender.join().unwrap();

    assert!(
        matches!(sent, Err(herdr_client::Error::Cancelled)),
        "{sent:?}"
    );
    assert_eq!(told, []);
    thread::sleep(Duration::from_secs(5));
    assert_eq!(written(folder.path(), "lines"), "");
}

#[test]
#[ignore = "needs herdr-core: hc on the PATH or TEST_HC_BIN_PATH"]
fn a_prompt_withdrawn_once_herdr_core_typed_it_reports_it_sent() {
    let core = PrivateCore::holding_unsent_typing_for(Duration::ZERO);
    let folder = tempfile::tempdir().unwrap();
    let client = core.client();
    let agent = core.run(folder.path(), &line_agent_keeping_its_line(""));
    wait_for(&client, &agent, AgentStatus::Idle);
    // herdr-core types in a terminal that just started only once its output settled.
    thread::sleep(Duration::from_secs(3));
    let typed = {
        let waiting = core.command(&["wait", &agent.0, "--output", PROMPT, "--json"]);
        thread::spawn(move || {
            let mut waiting = waiting;
            waiting.output().unwrap()
        })
    };
    let withdrawal = EventCanceller::default();

    let (sender, _) = send_withdrawn_by(&client, &agent, PROMPT, withdrawal.clone());
    typed.join().unwrap();
    withdrawal.cancel();
    let (sent, told) = sender.join().unwrap();

    sent.unwrap();
    assert_eq!(written(folder.path(), "lines"), format!("{PROMPT}\n"));
    assert_eq!(told, []);
}

#[test]
#[ignore = "needs herdr-core: hc on the PATH or TEST_HC_BIN_PATH"]
fn a_screen_with_no_input_line_holds_the_prompt() {
    let core = PrivateCore::holding_unsent_typing_for(Duration::ZERO);
    let folder = tempfile::tempdir().unwrap();
    let client = core.client();
    let agent = core.run(
        folder.path(),
        &line_agent(&format!(
            r"stty -echo -icanon; printf '❯ 1. Yes\r\n  2. No\r\n'; {} IFS= read -rsn1 answer;",
            report("idle")
        )),
    );
    wait_for(&client, &agent, AgentStatus::Idle);

    let (sender, waits) = send_telling_waits(&client, &agent, PROMPT);
    assert_eq!(waits.recv().unwrap(), Some(PromptWait::NoInputLine));
    thread::sleep(Duration::from_secs(5));
    assert_eq!(written(folder.path(), "lines"), "");
    core.hc(&["keys", &agent.0, "1"]);
    let (sent, told) = sender.join().unwrap();

    sent.unwrap();
    assert_eq!(written(folder.path(), "lines"), format!("{PROMPT}\n"));
    assert_eq!(told, [Some(PromptWait::NoInputLine), None]);
}

#[test]
#[ignore = "needs herdr-core: hc on the PATH or TEST_HC_BIN_PATH"]
fn a_prompt_herdr_core_does_not_type_at_once_is_withdrawn_and_typed_once_on_an_empty_line() {
    let core = PrivateCore::holding_unsent_typing_for(Duration::ZERO);
    let folder = tempfile::tempdir().unwrap();
    let client = core.client();
    let agent = core.run(folder.path(), &line_agent(""));
    wait_for(&client, &agent, AgentStatus::Idle);
    let mut person = core.person_at(&agent);
    // herdr-core waits out its quiet pause after a key, even one erased, before it types a send.
    person.types("x", Duration::ZERO);
    person.presses_backspace();
    wait_until_read(folder.path(), '\u{7f}');

    let (sender, waits) = send_telling_waits(&client, &agent, PROMPT);
    thread::sleep(Duration::from_millis(500));
    person.types("Re", Duration::from_millis(50));
    assert_eq!(waits.recv().unwrap(), Some(PromptWait::TypedText));
    // herdr-core types a held send about 3 s after the last key.
    thread::sleep(Duration::from_secs(6));
    assert_eq!(written(folder.path(), "lines"), "");
    person.erases_line();
    let (sent, told) = sender.join().unwrap();

    sent.unwrap();
    assert_eq!(written(folder.path(), "lines"), format!("{PROMPT}\n"));
    assert_eq!(told, [Some(PromptWait::TypedText), None]);
}

#[test]
#[ignore = "needs herdr-core: hc on the PATH or TEST_HC_BIN_PATH"]
fn a_prompt_to_an_agent_whose_line_is_not_read_waits() {
    let core = PrivateCore::holding_unsent_typing_for(Duration::ZERO);
    let folder = tempfile::tempdir().unwrap();
    let client = core.client();
    let agent = core.run(
        folder.path(),
        &format!(
            r#"{} printf '\033]7501;app=pi:state=idle\007'; read -r line; printf '%s' "$line" > typed"#,
            input_line()
        ),
    );
    wait_for(&client, &agent, AgentStatus::Idle);

    let (_sender, waits) = send_telling_waits(&client, &agent, PROMPT);

    assert_eq!(
        waits.recv().unwrap(),
        Some(PromptWait::UnreadAgent("pi".to_owned()))
    );
    thread::sleep(Duration::from_secs(4));
    assert_eq!(written(folder.path(), "typed"), "");
}

#[test]
#[ignore = "needs herdr-core: hc on the PATH or TEST_HC_BIN_PATH"]
fn a_key_pressed_after_herdr_core_typed_the_prompt_never_sends_it_again() {
    let core = PrivateCore::holding_unsent_typing_for(Duration::ZERO);
    let folder = tempfile::tempdir().unwrap();
    let client = core.client();
    let agent = core.run(folder.path(), &line_agent_without_turns(""));
    wait_for(&client, &agent, AgentStatus::Idle);
    // herdr-core types in a terminal that just started only once its output settled.
    thread::sleep(Duration::from_secs(3));
    let mut person = core.person_at(&agent);
    let typed = {
        let waiting = core.command(&["wait", &agent.0, "--output", "Review", "--json"]);
        thread::spawn(move || {
            let mut waiting = waiting;
            waiting.output().unwrap()
        })
    };

    let (sender, _) = send_telling_waits(&client, &agent, PROMPT);
    typed.join().unwrap();
    person.types("next", Duration::from_millis(50));
    let (sent, told) = sender.join().unwrap();

    sent.unwrap();
    assert_eq!(written(folder.path(), "lines"), format!("{PROMPT}\n"));
    assert_eq!(
        told,
        [],
        "the client took the person's keys for a reason to send again"
    );
}

#[test]
#[ignore = "needs herdr-core: hc on the PATH or TEST_HC_BIN_PATH"]
fn a_dialog_opened_while_herdr_core_waits_out_its_pause_withdraws_the_prompt() {
    let core = PrivateCore::holding_unsent_typing_for(Duration::ZERO);
    let folder = tempfile::tempdir().unwrap();
    let client = core.client();
    let agent = core.run(folder.path(), &line_agent(""));
    wait_for(&client, &agent, AgentStatus::Idle);
    thread::sleep(Duration::from_secs(3));
    let mut person = core.person_at(&agent);
    // herdr-core waits out its quiet pause after a key, even one erased, before it types a send.
    person.types("x", Duration::ZERO);
    person.presses_backspace();
    wait_until_read(folder.path(), '\u{7f}');

    let (sender, waits) = send_telling_waits(&client, &agent, PROMPT);
    thread::sleep(Duration::from_millis(500));
    person.types("\u{12}", Duration::ZERO);
    assert_eq!(waits.recv().unwrap(), Some(PromptWait::NoInputLine));
    thread::sleep(Duration::from_secs(5));
    assert_eq!(written(folder.path(), "dialog"), "");
    person.presses_esc();
    let (sent, told) = sender.join().unwrap();

    sent.unwrap();
    assert_eq!(written(folder.path(), "dialog"), "");
    assert_eq!(written(folder.path(), "lines"), format!("{PROMPT}\n"));
    assert_eq!(told, [Some(PromptWait::NoInputLine), None]);
}

#[test]
#[ignore = "needs herdr-core: hc on the PATH or TEST_HC_BIN_PATH"]
fn an_antigravity_agent_takes_a_prompt_only_on_an_empty_line_and_starts_on_it() {
    let core = PrivateCore::holding_unsent_typing_for(Duration::ZERO);
    let folder = tempfile::tempdir().unwrap();
    let client = core.client();
    // agy 1.5.18: OSC 7501 app=antigravity, rule-coloured placeholder, cursor hidden (`?25l`) on panels.
    let script = r#"stty -echo -icanon; typed=''; mode=line
draw() {
  if [ "$mode" = line ]; then
    printf '\033[?25h\033[H\033[2J\033[38;5;8m────────────────────────\033[0m\r\n'
    if [ -z "$typed" ]; then
      printf '\033[38;5;12m> \033[0m\033[38;5;8mAuto mode: edits & safe commands auto-approved\033[0m\r\n'
    else
      printf '\033[38;5;12m> \033[0m%s\r\n' "$typed"
    fi
    printf '\033[38;5;8m────────────────────────\033[0m\r\n'
  else
    printf '\033[?25l\033[H\033[2J\033[38;5;8m────────────────────────\033[0m\r\n\033[38;5;12m>\033[0m\r\n\033[38;5;8m────────────────────────\033[0m\r\nAntigravity CLI help\r\n'
  fi
}
draw; printf '\033]7501;state=idle:app=antigravity\033\\'
while IFS= read -rsn1 -d '' key; do
  printf '%s' "$key" >> keys
  if [ "$mode" = panel ]; then
    if [ "$key" = $'\x1b' ]; then mode=line; draw; else printf '%s' "$key" >> panel; fi
    continue
  fi
  case "$key" in
    $'\n') printf '%s\n' "$typed" >> lines; typed=''; printf '\033]7501;state=working:app=antigravity\033\\'; draw; printf '\033]7501;state=done:app=antigravity\033\\' ;;
    $'\x12') mode=panel ;;
    $'\x15') typed='' ;;
    $'\x7f') typed="${typed%?}" ;;
    *) typed+="$key" ;;
  esac
  draw
done"#;
    let agent = core.run(folder.path(), script);
    wait_for(&client, &agent, AgentStatus::Idle);
    assert_eq!(
        client
            .get_agent(&agent)
            .unwrap()
            .and_then(|listed| listed.agent)
            .as_deref(),
        Some("antigravity")
    );
    client
        .prompt_agent(&agent, "First prompt to Antigravity")
        .unwrap();
    assert_eq!(
        written(folder.path(), "lines"),
        "First prompt to Antigravity\n"
    );

    let mut person = core.person_at(&agent);
    person.types("zzdraft", Duration::from_millis(50));
    wait_until_read(folder.path(), 'z');
    let (sender, waits) = send_telling_waits(&client, &agent, "Second prompt");
    assert_eq!(waits.recv().unwrap(), Some(PromptWait::TypedText));
    person.erases_line();
    let (sent, _) = sender.join().unwrap();
    sent.unwrap();
    assert_eq!(
        written(folder.path(), "lines"),
        "First prompt to Antigravity\nSecond prompt\n"
    );

    core.hc(&["keys", &agent.0, "ctrl+r"]);
    wait_until_read(folder.path(), '\x12');
    let (sender, waits) = send_telling_waits(&client, &agent, "Third prompt");
    assert_eq!(waits.recv().unwrap(), Some(PromptWait::NoInputLine));
    person.presses_esc();
    let (sent, told) = sender.join().unwrap();
    sent.unwrap();
    assert_eq!(written(folder.path(), "panel"), "");
    assert_eq!(
        written(folder.path(), "lines"),
        "First prompt to Antigravity\nSecond prompt\nThird prompt\n"
    );
    assert_eq!(told, [Some(PromptWait::NoInputLine), None]);
}

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
            "{} {} read -r line; printf '%s' \"$line\" > typed; {} read -r line",
            input_line(),
            report("idle"),
            report("done")
        ),
    );
    wait_for(&client, &agent, AgentStatus::Idle);

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
