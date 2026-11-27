use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use hc_client::HcClient;
use hc_test_support::PrivateCore;
use herdr_bus::{Bus, MessageQuery};
use herdr_client::client::EventCanceller;
use herdr_client::protocol::{
    AgentHost, AgentPort, AgentStatus, AgentTarget, PaneId, PromptWait, TabId,
};
use review_mcp::Endpoint;
use review_mcp_config::{Client, UserConfig};
use review_source::{AnchorKind, DiffRangeAnchor};
use review_store::ReviewStore;
use review_test_support::TestPort;
use review_thread_service::{Command as ThreadServiceCommand, Event, Worker};
use review_threads::{Author, Post, ThreadCommand};

const PACE: Duration = Duration::from_millis(150);
const TURN_TIMEOUT: Duration = Duration::from_secs(60);

#[test]
#[ignore = "the contract check under herdr-core: real Antigravity CLI turns on the owner's Google AI Pro login; see the real-agent-checks skill"]
fn antigravity_takes_the_reviewer_s_prompts_and_replies_through_mcp_and_herdr_bus() {
    let bridge = Path::new(env!("CARGO_BIN_EXE_reviewer-control")).with_file_name("reviewer-mcp");
    assert!(
        bridge.is_file(),
        "build reviewer-mcp first: cargo build -p review-mcp-config --bin reviewer-mcp"
    );
    let port = TestPort::new();
    let agent = RealAntigravity::start(&bridge, port.number());

    review_comment_round_trip_and_bus(&agent, port.number());
    panel_hiding_the_cursor_holds_the_prompt(&agent);
    text_left_in_the_box_holds_the_prompt(&agent);
    agent.exit();
}

fn review_comment_round_trip_and_bus(agent: &RealAntigravity, port: u16) {
    let store_dir = tempfile::tempdir().unwrap();
    let store = ReviewStore::open(store_dir.path().join("state"), store_dir.path()).unwrap();
    let bus_path = store_dir.path().join("bus.db");
    let endpoint = Endpoint::for_repository(&agent.repo, Some(port)).unwrap();
    let (tx, rx) = mpsc::channel();
    let worker = Worker::start(
        store,
        agent.client.clone(),
        None,
        AgentTarget::new(TabId("tb1".into()), Some(agent.terminal.clone())),
        Ok(endpoint),
        |_| Err("No Explore round in this contract check".into()),
        move |event| {
            let _ = tx.send(event);
        },
    );
    worker.publish_on_bus(Ok(bus_path.clone()));
    worker.send(ThreadServiceCommand::Thread(ThreadCommand::Load(
        "agy-contract".into(),
    )));
    let comment_text =
        "Reply with only the word FIG using the herdr_reviewer reply tool. Do not edit any files.";
    worker.send(ThreadServiceCommand::Thread(ThreadCommand::Post {
        review_unit: "agy-contract".into(),
        post: Post::start(anchor(), "+fn demo() {}\n".into(), comment_text.into()),
    }));

    let reply = wait_for_agent_reply(agent, &rx);
    assert!(
        reply.to_ascii_uppercase().contains("FIG"),
        "expected agent reply to contain FIG, got: {reply:?}"
    );
    worker.prompt_sender().flush();
    let bus = Bus::open(&bus_path).unwrap();
    let messages = bus
        .messages(&MessageQuery {
            output: Some(review_words_bus::OUTPUT.into()),
            ..MessageQuery::default()
        })
        .unwrap();
    assert_eq!(messages.len(), 1, "expected one words message on herdr-bus");
    let data = &messages[0].message.data;
    assert_eq!(data["text"].as_str(), Some(comment_text));
    assert_eq!(
        data["to"].as_str(),
        Some(format!("pane:{}", agent.terminal.0).as_str())
    );
    agent.wait_for_prompt();
}

fn wait_for_agent_reply(agent: &RealAntigravity, rx: &mpsc::Receiver<Event>) -> String {
    let stop_approver = EventCanceller::default();
    let approver = spawn_mcp_tool_approver(agent, stop_approver.clone());
    let deadline = Instant::now() + TURN_TIMEOUT;
    let reply = loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let event = rx.recv_timeout(left).unwrap_or_else(|_| {
            panic!(
                "Assumption: Antigravity receives the wakeup, calls get_new_messages and reply. \
                 Status: {:?}\nTerminal:\n{}",
                agent.client.get_agent(&agent.terminal),
                agent.screen()
            )
        });
        if let Some(reply) = agent_reply_from(event, agent) {
            break reply;
        }
    };
    stop_approver.cancel();
    let _ = approver.join();
    reply
}

fn spawn_mcp_tool_approver(
    agent: &RealAntigravity,
    canceller: EventCanceller,
) -> thread::JoinHandle<()> {
    let client = agent.client.clone();
    let terminal = agent.terminal.clone();
    let mut keys_command = agent.core.command(&["keys", &agent.terminal.0, "enter"]);
    let mut read_command = agent.core.command(&["read", &agent.terminal.0]);
    thread::spawn(move || {
        let _ = client.forward_agent_status(&terminal, &canceller, &mut |status| {
            if status == AgentStatus::Blocked {
                let deadline = Instant::now() + Duration::from_secs(5);
                while !canceller.is_cancelled() && Instant::now() < deadline {
                    if read_command.output().is_ok_and(|output| {
                        String::from_utf8_lossy(&output.stdout).contains("Allow calling this tool?")
                    }) {
                        let _ = keys_command.output();
                        break;
                    }
                    thread::sleep(Duration::from_millis(100));
                }
            }
            true
        });
    })
}

fn agent_reply_from(event: Event, agent: &RealAntigravity) -> Option<String> {
    match event {
        Event::Loaded(loaded) => loaded.result.ok().and_then(|book| {
            book.threads()
                .iter()
                .flat_map(|thread| thread.messages.iter())
                .find(|message| message.author == Author::Agent)
                .map(|message| message.text.clone())
        }),
        Event::Error(error) => panic!("thread service error: {error}"),
        Event::Wakeup {
            notice: Some(review_threads::WakeupNotice::Failed(failure)),
            ..
        } => panic!(
            "wakeup failed: {}. Terminal:\n{}",
            failure.error,
            agent.screen()
        ),
        _ => None,
    }
}

fn panel_hiding_the_cursor_holds_the_prompt(agent: &RealAntigravity) {
    let help_shown = agent.output("Quick Reference");
    agent
        .core
        .hc(&["keys", &agent.terminal.0, "/", "h", "e", "l", "p", "enter"]);
    agent.printed(help_shown, "Antigravity opens the /help panel");
    let replies = agent.output("LIME");
    let (waits, sender) = agent.send_telling_waits("Reply with only the word lime, in capitals.");
    assert_eq!(
        waits.recv().unwrap(),
        Some(PromptWait::NoInputLine),
        "Assumption: /help hides the cursor so Antigravity shows no active input line. \
         Terminal:\n{}",
        agent.screen()
    );
    agent.core.hc(&["keys", &agent.terminal.0, "esc"]);
    sender.join().unwrap().unwrap();
    agent.printed(replies, "Antigravity runs the prompt after /help closes");
    agent.wait_for_prompt();
}

fn text_left_in_the_box_holds_the_prompt(agent: &RealAntigravity) {
    let mut person = agent.core.person_at(&agent.terminal);
    let echoed = agent.output("half");
    person.types("half", PACE);
    agent.printed(echoed, "Antigravity draws the person's typed text");
    let replies = agent.output("KIWI");
    let (waits, sender) = agent.send_telling_waits("Reply with only the word kiwi, in capitals.");
    assert_eq!(
        waits.recv().unwrap(),
        Some(PromptWait::TypedText),
        "Assumption: the client reads typed text in Antigravity's input box. Terminal:\n{}",
        agent.screen()
    );
    // Past herdr-core's quiet pause after the person's last key, which alone holds a send.
    thread::sleep(Duration::from_secs(6));
    assert!(
        agent.screen().contains("half"),
        "the person's text stays in the box, with nothing typed over it"
    );
    person.erases_line();
    drop(person);
    sender.join().unwrap().unwrap();
    agent.printed(replies, "Antigravity runs the prompt once cleared");
    agent.wait_for_prompt();
}

fn anchor() -> DiffRangeAnchor {
    DiffRangeAnchor {
        source_checkpoint: "initial".into(),
        old_path: None,
        new_path: Some("lib.rs".into()),
        old_lines: None,
        new_lines: Some(0..1),
        target_kind: AnchorKind::Lines,
        source_hunk_count: 1,
        old_content: None,
        new_content: Some(b"fn demo() {}\n".to_vec()),
        diff_hash: "hash".into(),
    }
}

struct RealAntigravity {
    client: HcClient,
    terminal: PaneId,
    core: PrivateCore,
    repo: PathBuf,
    _scratch_home: tempfile::TempDir,
}

impl RealAntigravity {
    fn start(bridge: &Path, port: u16) -> Self {
        let core = PrivateCore::start();
        let scratch_home = tempfile::tempdir().unwrap();
        assert!(
            UserConfig::new(
                Client::Antigravity,
                &scratch_home.path().join(".gemini/config"),
                bridge.to_owned(),
            )
            .install()
            .unwrap()
        );
        let repo = scratch_repo();
        let mut argv = vec![
            "bash".to_owned(),
            "-c".to_owned(),
            "read -r; exec \"$@\"".to_owned(),
            "bash".to_owned(),
        ];
        argv.extend(antigravity_environment(scratch_home.path(), port));
        argv.extend(["agy", "--effort", "low"].map(String::from));
        let argv_refs: Vec<&str> = argv.iter().map(String::as_str).collect();
        let terminal = core.start_in(&repo, &argv_refs);
        let agent = Self {
            client: core.client(),
            terminal,
            core,
            repo,
            _scratch_home: scratch_home,
        };
        let theme = agent.output("Choose your color scheme");
        agent.core.hc(&["keys", &agent.terminal.0, "enter"]);
        let _ = theme.wait_with_output().unwrap();
        agent.accept_onboarding();
        agent.wait_for_prompt();
        assert_eq!(
            agent
                .client
                .get_agent(&agent.terminal)
                .unwrap()
                .and_then(|live| live.agent)
                .as_deref(),
            Some("antigravity")
        );
        agent
    }

    fn send_telling_waits(
        &self,
        text: &'static str,
    ) -> (
        mpsc::Receiver<Option<PromptWait>>,
        thread::JoinHandle<herdr_client::Result<()>>,
    ) {
        let (client, terminal) = (self.client.clone(), self.terminal.clone());
        let (told, waits) = mpsc::channel();
        let sender = thread::spawn(move || {
            client.submit_prompt_telling_waits(
                &terminal,
                text,
                &mut |wait| {
                    let _ = told.send(wait);
                },
                &EventCanceller::default(),
            )
        });
        (waits, sender)
    }

    fn output(&self, text: &str) -> Child {
        self.core
            .command(&["wait", &self.terminal.0, "--output", text, "--json"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap()
    }

    fn printed(&self, output: Child, assumption: &str) {
        let ended = output.wait_with_output().unwrap();
        assert!(
            String::from_utf8_lossy(&ended.stdout).contains("\"output\""),
            "Assumption: {assumption}: {ended:?}. Terminal:\n{}",
            self.screen()
        );
    }

    // Antigravity's onboarding dialogs all report `state=blocked`, so `hc wait --output` returns
    // immediately while blocked and the TUI tells no one when it takes keys (agy 1.5.18, hc 0.1.20).
    fn accept_onboarding(&self) {
        let deadline = Instant::now() + Duration::from_secs(45);
        while Instant::now() < deadline {
            if self
                .client
                .get_agent(&self.terminal)
                .unwrap()
                .is_some_and(|live| live.agent_status.waits_for_prompt())
            {
                return;
            }
            let screen = self.screen();
            if screen.contains("Terms of Service") {
                let keys: &[&str] = if screen.contains(">  Done") {
                    &["keys", &self.terminal.0, "enter"]
                } else {
                    &["keys", &self.terminal.0, "down", "right"]
                };
                self.core.hc(keys);
            } else if screen.contains("Choose your color scheme") || screen.contains("Yes, I trust")
            {
                self.core.hc(&["keys", &self.terminal.0, "enter"]);
            }
            thread::sleep(Duration::from_millis(200));
        }
        panic!(
            "Antigravity did not finish onboarding. Terminal:\n{}",
            self.screen()
        );
    }

    fn wait_for_prompt(&self) {
        let mut last = None;
        let _ = self.client.forward_agent_status(
            &self.terminal,
            &EventCanceller::default(),
            &mut |status| {
                last = Some(status);
                !status.waits_for_prompt()
            },
        );
        assert!(
            last.is_some_and(AgentStatus::waits_for_prompt),
            "Antigravity did not return to idle/done: {last:?}. Terminal:\n{}",
            self.screen()
        );
    }

    fn exit(&self) {
        self.core
            .hc(&["keys", &self.terminal.0, "/", "e", "x", "i", "t", "enter"]);
    }

    fn screen(&self) -> String {
        self.core.screen(&self.terminal)
    }
}

fn antigravity_environment(scratch_home: &Path, port: u16) -> Vec<String> {
    let mut unset = Vec::new();
    let mut set = vec![
        format!("HOME={}", scratch_home.display()),
        format!("HERDR_REVIEWER_MCP_PORT={port}"),
    ];
    for (name, _) in std::env::vars_os() {
        if let Some(name) = name.to_str()
            && name.starts_with("ANTIGRAVITY_")
        {
            unset.extend(["-u".to_owned(), name.to_owned()]);
        }
    }
    for name in ["XDG_DATA_HOME", "XDG_STATE_HOME", "XDG_CONFIG_HOME"] {
        match std::env::var(name) {
            Ok(owner_s) => set.push(format!("{name}={owner_s}")),
            Err(_) => unset.extend(["-u".to_owned(), name.to_owned()]),
        }
    }
    [vec!["env".to_owned()], unset, set].concat()
}

fn scratch_repo() -> PathBuf {
    let cache = std::env::var_os("XDG_CACHE_HOME").map_or_else(
        || PathBuf::from(std::env::var_os("HOME").unwrap()).join(".cache"),
        PathBuf::from,
    );
    let repo = cache.join("herdr-reviewer").join("agy-hc-contract-check");
    let _ = fs::remove_dir_all(&repo);
    fs::create_dir_all(&repo).unwrap();
    let status = Command::new("jj")
        .args(["git", "init"])
        .current_dir(&repo)
        .status()
        .unwrap();
    assert!(status.success(), "jj git init failed");
    fs::write(repo.join("lib.rs"), "fn demo() {}\n").unwrap();
    repo
}
