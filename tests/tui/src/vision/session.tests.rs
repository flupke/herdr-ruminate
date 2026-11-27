use fake_agent::Reported;
use serde_json::Value;

use super::*;

const GUARD: Duration = Duration::from_secs(30);

struct Fixture {
    session: Session,
    vision: Vision,
    _directory: tempfile::TempDir,
}

impl Fixture {
    fn start() -> (Self, Screen) {
        Self::start_with(true)
    }

    // The bubblewrap sandbox does not show the host's `/tmp`.
    fn start_with(agent_hooks: bool) -> (Self, Screen) {
        let sessions = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/vision-tests");
        fs::create_dir_all(&sessions).unwrap();
        let directory = tempfile::tempdir_in(sessions).unwrap();
        let reviewer = std::env::var_os("REVIEWER_BIN_PATH")
            .expect("run with make e2e-tui, which builds the reviewer");
        let (session, vision, screen) = Session::start(
            reviewer.into(),
            &StartOptions {
                repository: RepoType::Jj,
                seed: Seed::sample(),
                agent_hooks,
            },
            directory.path().join("session"),
        )
        .unwrap();
        let fixture = Self {
            session,
            vision,
            _directory: directory,
        };
        (fixture, screen)
    }

    fn keys(&mut self, keys: &[&str]) -> Screen {
        let keys: Vec<String> = keys.iter().map(|key| (*key).to_owned()).collect();
        self.session.focus_reviewer(&self.vision).unwrap();
        let (_, screen) = self.vision.act(&Input::Keys(&keys)).unwrap();
        screen.unwrap()
    }

    fn paste(&mut self, text: &str) -> Screen {
        self.session.focus_reviewer(&self.vision).unwrap();
        let (_, screen) = self.vision.act(&Input::Paste(text)).unwrap();
        screen.unwrap()
    }

    fn wait_for(&self, text: &str) -> Screen {
        self.vision
            .wait_for(text, false, GUARD, Some(&self.session.terminals.reviewer))
            .unwrap()
    }

    fn ask(&mut self) {
        let kickoff = self.session.next_turn(None, GUARD).unwrap();
        assert_eq!(kickoff["kind"], "kickoff", "{kickoff}");
        let question: Value =
            serde_json::from_str(include_str!("../../examples/question.json")).unwrap();
        let (reply, screen) = self
            .session
            .reply(
                &mut self.vision,
                kickoff["turn"].as_u64().unwrap(),
                "submit_question",
                question["arguments"].clone(),
            )
            .unwrap();
        assert_eq!(reply["isError"], false, "{reply}");
        assert!(screen.text.contains("Question 1"), "{}", screen.text);
    }
}

#[test]
fn each_action_returns_the_screen_of_the_reviewers_reaction() {
    let (mut fixture, _) = Fixture::start();
    let files = fixture.wait_for("math.rs");

    let help = fixture.keys(&["?"]);
    assert!(help.text.contains("Keyboard shortcuts"), "{}", help.text);
    assert!(help.frame > files.frame);
    let closed = fixture.keys(&["esc"]);
    assert!(
        !closed.text.contains("Keyboard shortcuts"),
        "{}",
        closed.text
    );
}

#[test]
fn the_fake_agent_answers_an_explore_round_in_the_pane() {
    let (mut fixture, _) = Fixture::start();
    fixture.wait_for("math.rs");
    fixture.keys(&["e", "p"]);
    fixture.ask();
}

#[test]
fn the_host_reaches_the_explore_page_the_action_opened_in_the_sandbox() {
    let (mut fixture, _) = Fixture::start();
    fixture.wait_for("math.rs");
    fixture.keys(&["e", "p"]);
    fixture.ask();
    let page = fixture.session.explore_page(&fixture.vision).unwrap();
    let port = loopback_port(&page).unwrap();
    let path = page.split_once(&format!(":{port}")).unwrap().1;
    assert_eq!(status_line(port, path), "HTTP/1.1 303 See Other", "{page}");
    let network = fixture.session.scratch.page_port();
    assert!(
        status_line(network, "/").starts_with("HTTP/1.1 403"),
        "port {network}"
    );
}

fn status_line(port: u16, path: &str) -> String {
    use std::io::Write;
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.set_read_timeout(Some(GUARD)).unwrap();
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"
    )
    .unwrap();
    let mut status = String::new();
    BufReader::new(stream).read_line(&mut status).unwrap();
    status.trim_end().to_owned()
}

#[test]
fn a_stopped_session_leaves_no_process_and_its_repository_goes() {
    let (fixture, _) = Fixture::start();
    let repository = fixture.session.scratch.root().to_owned();
    let Fixture {
        session,
        vision,
        _directory,
    } = fixture;
    drop(session);
    let stopped = vision.stop().unwrap();
    assert_eq!(stopped.processes, Vec::<u32>::new());
    assert!(!repository.exists());
}

#[test]
fn a_reopened_reviewer_starts_once_the_old_one_has_stopped() {
    let (mut fixture, _) = Fixture::start();
    fixture.wait_for("math.rs");
    let before = fixture.session.terminals.reviewer.clone();
    fixture
        .session
        .reopen(&mut fixture.vision, None, None)
        .unwrap();
    assert_ne!(fixture.session.terminals.reviewer, before);
    fixture.wait_for("math.rs");
    fixture.keys(&["e", "p"]);
    fixture.ask();
}

#[test]
fn marks_made_in_one_jj_workspace_show_in_another_workspace_of_the_repository() {
    let (mut fixture, _) = Fixture::start();
    fixture.wait_for("○ notes.md");
    fixture.keys(&["space"]);
    fixture.wait_for("✓ notes.md");
    let main = fixture.session.scratch.root().to_owned();

    fixture
        .session
        .reopen(&mut fixture.vision, None, Some("second"))
        .unwrap();

    assert_ne!(fixture.session.scratch.root(), main);
    let screen = fixture.wait_for("○ sample.rs");
    assert!(screen.text.contains("✓ notes.md"), "{}", screen.text);
}

#[test]
fn a_mark_painted_just_before_the_pane_closes_is_saved() {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    let (mut fixture, _) = Fixture::start();
    fixture.wait_for("○ notes.md");
    let real_jj = find_on_path("jj").expect("jj must be on PATH");
    let dir = fixture.session.directory().to_owned();
    let entered = dir.join("jj-entered");
    let release = dir.join("jj-release");
    let holding = dir.join("jj-holding");
    for fifo in [&entered, &release] {
        let status = std::process::Command::new("mkfifo")
            .arg(fifo)
            .status()
            .unwrap();
        assert!(status.success(), "mkfifo {fifo:?} failed");
    }
    let mut release_pipe = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&release)
        .unwrap();
    let entered_path = entered.clone();
    let (entered_tx, entered_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let mut reader = BufReader::new(fs::File::open(entered_path).unwrap());
        let _ = reader.read_line(&mut line);
        let _ = entered_tx.send(line);
    });
    let wrapper = dir.join("agent-bin/jj");
    fs::write(
        &wrapper,
        format!(
            "#!/bin/sh\nif mkdir '{}' 2>/dev/null; then\n  printf 'entered\\n' > '{}'\n  IFS= read -r _ < '{}'\nfi\nexec '{}' \"$@\"\n",
            holding.display(),
            entered.display(),
            release.display(),
            real_jj.display(),
        ),
    )
    .unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755)).unwrap();

    fixture.keys(&["space"]);
    fixture.wait_for("✓ notes.md");
    entered_rx
        .recv_timeout(GUARD)
        .expect("worker must start saving the mark");

    let pane = fixture.session.terminals.reviewer_pane.clone();
    let reviewer = fixture.session.terminals.reviewer.clone();
    do_command(&fixture.vision, "pane.close", &json!({"pane": pane})).unwrap();
    let _ = writeln!(release_pipe, "go");
    wait_terminal_ended(&fixture.vision, &reviewer).unwrap();
    fixture.session.terminals = open_reviewer(
        &fixture.vision,
        &fixture.session.scratch,
        &fixture.session.reviewer_program,
        fixture.session.terminals.agent.clone(),
        fixture.session.terminals.agent_pane.clone(),
    )
    .unwrap();
    fixture.session.first_screen(&fixture.vision).unwrap();
    let screen = fixture.wait_for("○ sample.rs");
    assert!(screen.text.contains("✓ notes.md"), "{}", screen.text);
}

fn find_on_path(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(program))
        .find(|candidate| candidate.is_file())
}

fn tool_json(result: &Value) -> Value {
    serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap()
}

#[test]
fn the_fake_agent_answers_a_review_comment_in_the_threads_tab() {
    let (mut fixture, _) = Fixture::start();
    fixture.wait_for("math.rs");
    fixture.keys(&["tab", "j", "j", "a"]);
    fixture.paste("Why this line?");
    fixture.keys(&["ctrl+enter"]);
    let Fixture {
        session, vision, ..
    } = &mut fixture;

    let wakeup = session.next_turn(None, GUARD).unwrap();
    assert_eq!(wakeup["kind"], "comments", "{wakeup}");
    let turn = wakeup["turn"].as_u64().unwrap();
    let (fetched, _) = session
        .reply(vision, turn, "get_new_messages", serde_json::json!({}))
        .unwrap();
    let thread = &tool_json(&fetched)["threads"][0];
    let (replied, _) = session
        .reply(
            vision,
            turn,
            "reply",
            serde_json::json!({
                "thread_id": thread["thread_id"],
                "in_reply_to": thread["in_reply_to"],
                "message_id": "6f1c2a9e-3b7d-4e5a-9c8f-1d2e3f4a5b6c",
                "text": "Because the fixture needs a third line.",
            }),
        )
        .unwrap();
    assert_eq!(replied["isError"], false, "{replied}");

    fixture.keys(&["t"]);
    let screen = fixture.wait_for("Because the fixture needs a");
    assert!(screen.text.contains("Why this line?"), "{}", screen.text);
}

#[test]
fn the_fake_agent_revises_the_open_question_in_the_pane() {
    let (mut fixture, _) = Fixture::start();
    fixture.wait_for("math.rs");
    fixture.keys(&["e", "p"]);
    fixture.ask();
    let question: Value =
        serde_json::from_str(include_str!("../../examples/question.json")).unwrap();
    let mut revised = question["arguments"]["update"]["next"].clone();
    revised["version"] = 2.into();
    revised["text"] = "Should the helpers reject negative input?".into();

    let (reply, _) = fixture
        .session
        .reply(
            &mut fixture.vision,
            1,
            "revise_question",
            serde_json::json!({"revision": {
                "message": "message-1", "reason": "You said only positive input reaches them.",
                "question": revised,
            }}),
        )
        .unwrap();

    assert_eq!(reply["isError"], false, "{reply}");
    let screen = fixture.wait_for("Should the helpers reject negative input?");
    assert!(
        screen.text.contains("Revised after your message"),
        "{}",
        screen.text
    );
}

#[test]
fn fork_submit_plays_a_fork_and_a_bare_answer_continues_as_it() {
    let (mut fixture, _) = Fixture::start();
    fixture.wait_for("math.rs");
    fixture.keys(&["e"]);
    let every = fixture.keys(&["z", "z"]);
    assert!(
        every
            .text
            .contains("Run ahead (experimental): every choice"),
        "{}",
        every.text
    );
    fixture.keys(&["p"]);
    fixture.ask();

    let forks = fixture.session.forks(Some(2), GUARD).unwrap();
    let mut choices: Vec<_> = forks["forks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|fork| fork["choice"].as_str().unwrap())
        .collect();
    choices.sort_unstable();
    assert_eq!(choices, ["keep", "saturate"], "{forks}");
    let own = forks["agent"]["session"].clone();
    let keep = fork_of(&forks, "keep")["session"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(fork_of(&forks, "keep")["parent"], own);
    let sandbox = fixture.vision.sandbox();
    let runtime = Path::new(sandbox.variable("XDG_RUNTIME_DIR").unwrap());
    let outside = sandbox
        .directory()
        .join(runtime.strip_prefix(sandbox.inside()).unwrap());
    assert!(outside.join("herdr-reviewer").exists());
    assert_ne!(
        Some(runtime.as_os_str()),
        std::env::var_os("XDG_RUNTIME_DIR").as_deref()
    );
    let conclusion: Value =
        serde_json::from_str(include_str!("../../examples/conclusion.json")).unwrap();
    let (reply, _) = fixture
        .session
        .fork_submit(
            &mut fixture.vision,
            &keep,
            "submit_conclusion",
            conclusion["arguments"].clone(),
        )
        .unwrap();
    assert_eq!(reply["isError"], false, "{reply}");
    let shown = fixture.session.forks(None, GUARD).unwrap();
    assert_eq!(fork_of(&shown, "keep")["submitted"], true, "{shown}");

    fixture.keys(&["1", "ctrl+enter"]);

    let prepared = fixture.wait_for("Prepared while you were thinking");
    assert!(
        prepared.text.contains("To be implemented"),
        "{}",
        prepared.text
    );
    wait_agent_session(&fixture.session, &keep);
    let moved = fixture.session.forks(None, GUARD).unwrap();
    assert_eq!(moved["agent"]["session"], keep.as_str(), "{moved}");
    assert_eq!(moved["agent"]["resumed"], serde_json::json!([keep]));
}

fn ask_with_run_ahead(fixture: &mut Fixture) {
    fixture.wait_for("math.rs");
    fixture.keys(&["e", "z", "z", "p"]);
    fixture.ask();
}

fn forks_of_the_question(fixture: &Fixture) -> (String, String) {
    let forks = fixture.session.forks(Some(2), GUARD).unwrap();
    let session_of = |choice: &str| {
        fork_of(&forks, choice)["session"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    (session_of("keep"), session_of("saturate"))
}

fn fork_of<'a>(forks: &'a Value, choice: &str) -> &'a Value {
    forks["forks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|fork| fork["choice"] == choice)
        .unwrap_or_else(|| panic!("no fork takes {choice}: {forks}"))
}

fn answer_bare_and_ask_again(fixture: &mut Fixture) -> Screen {
    fixture.keys(&["1", "ctrl+enter"]);
    ask_again(fixture)
}

fn ask_again(fixture: &mut Fixture) -> Screen {
    let wakeup = fixture.session.next_turn(None, GUARD).unwrap();
    assert_eq!(wakeup["kind"], "wakeup", "{wakeup}");
    let question: Value =
        serde_json::from_str(include_str!("../../examples/question.json")).unwrap();
    let mut arguments = question["arguments"].clone();
    let update = &mut arguments["update"];
    update["design"] = Value::Null;
    update["topics"][0]["id"] = "rest".into();
    update["next"]["id"] = "q2".into();
    update["next"]["topic"] = "rest".into();
    update["next"]["text"] = "Should the helpers check their input?".into();
    update["interpretation"] =
        serde_json::json!({"status": "accepted", "recap": "Kept.", "follow_ups": []});
    update["reply"] = serde_json::json!({"text": "Kept as written.", "evidence": []});
    let (reply, _) = fixture
        .session
        .reply(
            &mut fixture.vision,
            wakeup["turn"].as_u64().unwrap(),
            "submit_question",
            arguments,
        )
        .unwrap();
    assert_eq!(reply["isError"], false, "{reply}");
    fixture.wait_for("Should the helpers check their input?")
}

#[test]
fn fork_end_ends_one_fork_without_a_turn_and_crashes_the_other() {
    let (mut fixture, _) = Fixture::start();
    ask_with_run_ahead(&mut fixture);
    let (keep, saturate) = forks_of_the_question(&fixture);

    fixture
        .session
        .fork_end(&mut fixture.vision, &keep, false)
        .unwrap();
    fixture
        .session
        .fork_end(&mut fixture.vision, &saturate, true)
        .unwrap();

    let forks = fixture.session.forks(None, GUARD).unwrap();
    let ended = |choice: &str| {
        let fork = fork_of(&forks, choice);
        (
            fork["submitted"].clone(),
            fork["finished"].clone(),
            fork["exited"].clone(),
        )
    };
    assert_eq!(
        ended("keep"),
        (false.into(), true.into(), true.into()),
        "{forks}"
    );
    assert_eq!(
        ended("saturate"),
        (false.into(), false.into(), true.into()),
        "{forks}"
    );
    let log = forks["log"].as_str().unwrap();
    assert!(log.contains("exit status: 1"), "{log}");
    let screen = answer_bare_and_ask_again(&mut fixture);
    assert!(
        screen
            .text
            .contains("Not prepared: the turn for this choice did not finish"),
        "{}",
        screen.text
    );
}

#[test]
fn a_switch_heard_late_still_continues_as_the_fork() {
    let (mut fixture, _) = Fixture::start();
    ask_with_run_ahead(&mut fixture);
    let (keep, _) = forks_of_the_question(&fixture);
    submit_conclusion(&mut fixture, &keep);
    fixture
        .session
        .switch_unheard(Some(Duration::from_secs(2)))
        .unwrap();

    fixture.keys(&["1", "ctrl+enter"]);

    let agent = Role::agent(AGENT);
    let from_agent = |received: &[Reported], event: &Event| {
        received
            .iter()
            .any(|reported| reported.from == agent && reported.event == *event)
    };
    let session = &fixture.session;
    let read = Event::ResumeReceived {
        session: keep.clone(),
    };
    session
        .events
        .try_wait_until("the agent's /resume", GUARD, |received| {
            from_agent(received, &read)
        })
        .unwrap();
    let read_at = std::time::Instant::now();
    wait_agent_session(session, &keep);
    assert!(read_at.elapsed() >= Duration::from_secs(1));
    fixture.wait_for("Prepared while you were thinking");
}

#[test]
fn an_unheard_switch_fails_and_retry_reaches_the_agent_once() {
    let (mut fixture, _) = Fixture::start();
    ask_with_run_ahead(&mut fixture);
    let (keep, _) = forks_of_the_question(&fixture);
    submit_conclusion(&mut fixture, &keep);
    fixture.session.switch_unheard(None).unwrap();
    fixture.keys(&["1", "ctrl+enter"]);
    let session = &fixture.session;

    let own = session.forks(None, GUARD).unwrap()["agent"]["session"]
        .as_str()
        .unwrap()
        .to_owned();
    let back = Event::ResumeReceived {
        session: own.clone(),
    };
    session
        .events
        .try_wait_until(
            "the agent back on its session",
            Duration::from_secs(40),
            |received| received.iter().any(|reported| reported.event == back),
        )
        .unwrap();
    wait_agent_session(session, &own);
    fixture.keys(&["r"]);

    let screen = ask_again(&mut fixture);
    assert!(
        screen
            .text
            .contains("Not prepared: the agent could not switch to the prepared turn"),
        "{}",
        screen.text
    );
    fixture.keys(&["1"]);
    fixture.paste("With a comment.");
    fixture.keys(&["ctrl+enter"]);
    let next = fixture.session.next_turn(None, GUARD).unwrap();
    assert_eq!(next["answer"]["question"]["id"], "q2", "{next}");
    let sessions = fixture.session.forks(None, GUARD).unwrap()["agent"].clone();
    assert_eq!(
        sessions["resumed"],
        serde_json::json!([keep, own]),
        "{sessions}"
    );
}

#[test]
#[ignore = "flupke/herdr-ruminate#22: under herdr-core, run-ahead continues as the fork \
            though hc reports the agent working at the answer"]
fn an_agent_shown_working_at_the_answer_takes_it_itself() {
    let (mut fixture, _) = Fixture::start();
    ask_with_run_ahead(&mut fixture);
    forks_of_the_question(&fixture);

    fixture
        .session
        .agent_working(&mut fixture.vision, true)
        .unwrap();
    fixture.keys(&["1", "ctrl+enter"]);
    fixture
        .session
        .agent_working(&mut fixture.vision, false)
        .unwrap();

    let screen = ask_again(&mut fixture);
    assert!(
        screen.text.contains("Not prepared: the agent was working"),
        "{}",
        screen.text
    );
}

#[test]
fn a_prompt_typed_to_the_agent_moves_its_conversation_past_the_forks() {
    let (mut fixture, _) = Fixture::start();
    ask_with_run_ahead(&mut fixture);
    forks_of_the_question(&fixture);
    let swallow = fixture.session.scratch.agent_directory().swallow();
    fs::write(&swallow, "").unwrap();

    fixture
        .session
        .agent_prompt(&mut fixture.vision, "What are you doing?")
        .unwrap();
    fs::remove_file(&swallow).unwrap();

    let screen = answer_bare_and_ask_again(&mut fixture);
    assert!(
        screen
            .text
            .contains("Not prepared: the agent's conversation moved on"),
        "{}",
        screen.text
    );
}

#[test]
fn an_agent_without_the_plugin_hooks_is_never_forked() {
    let (mut fixture, _) = Fixture::start_with(false);
    ask_with_run_ahead(&mut fixture);

    let screen = answer_bare_and_ask_again(&mut fixture);

    assert!(
        screen
            .text
            .contains("Not prepared: nothing was prepared for this question"),
        "{}",
        screen.text
    );
    assert_eq!(
        fixture.session.forks(None, GUARD).unwrap()["forks"],
        serde_json::json!([])
    );
}

fn submit_conclusion(fixture: &mut Fixture, session: &str) {
    let conclusion: Value =
        serde_json::from_str(include_str!("../../examples/conclusion.json")).unwrap();
    let (reply, _) = fixture
        .session
        .fork_submit(
            &mut fixture.vision,
            session,
            "submit_conclusion",
            conclusion["arguments"].clone(),
        )
        .unwrap();
    assert_eq!(reply["isError"], false, "{reply}");
}

fn wait_agent_session(session: &Session, started: &str) {
    session
        .events
        .try_wait_until("the agent on the session", GUARD, |received| {
            forks::agent_sessions(received).session.as_deref() == Some(started)
        })
        .unwrap();
}

#[test]
fn a_loopback_address_gives_its_port() {
    assert_eq!(
        loopback_port("http://127.0.0.1:8790/?token=abc").unwrap(),
        8790
    );
    assert_eq!(
        loopback_port("http://127.0.0.1:41000?token=abc").unwrap(),
        41000
    );
    assert!(loopback_port("http://192.168.1.2:8790/?token=abc").is_err());
}
