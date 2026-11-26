use std::fs;
use std::path::PathBuf;
use std::process::{Child, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use agent_hooks::{AgentHooks, Heard, HookDirectory, HookedAgent, OtherPrompts};
use hc_client::HcClient;
use hc_test_support::PrivateCore;
use herdr_client::client::EventCanceller;
use herdr_client::protocol::{AgentHost, AgentPort, AgentStatus, PaneId, PromptWait};

const PLUGIN: &str = "progressive-reviewer@herdr-progressive-reviewer";
const MODEL: &str = "claude-sonnet-5-5";
const HOOK_LIMIT: Duration = Duration::from_secs(5);
const BLOCKED: &str = "The progressive reviewer's contract check stopped this prompt.";
const PACE: Duration = Duration::from_millis(150);

#[test]
#[ignore = "the contract check under herdr-core: real Claude Code turns on the owner's login; see the real-agent-checks skill"]
fn claude_code_takes_the_reviewer_s_prompts_through_herdr_core() {
    let agent = RealAgent::start();
    let hooked = agent.hooked();

    a_prompt_starts_the_agent(&agent);
    a_long_prompt_is_heard_as_typed(&agent, &hooked);
    nothing_is_typed_while_a_question_waits(&agent);
    // herdr-core refuses to type in a terminal younger than its startup age limit until its output
    // settles, which the person's keys keep from happening.
    the_user_s_prompt_goes_on_while_the_reviewer_s_waits(&agent, &hooked);
    text_left_in_the_box_holds_the_prompt(&agent, &hooked);
}

fn the_user_s_prompt_goes_on_while_the_reviewer_s_waits(agent: &RealAgent, hooked: &HookedAgent) {
    let cherry = "Reply with only the word cherry, in capitals";
    let expectation = agent
        .hooks
        .expect_prompt(hooked, cherry, BLOCKED, agent.others());
    let replies = agent.output("CHERRY");
    let mut person = agent.core.person_at(&agent.terminal);
    person.types("Write", PACE);
    let sender = {
        let (client, terminal) = (agent.client.clone(), agent.terminal.clone());
        thread::spawn(move || client.submit_prompt(&terminal, cherry))
    };
    let hook = {
        let (client, terminal) = (agent.client.clone(), agent.terminal.clone());
        thread::spawn(move || {
            let heard = expectation.wait(Duration::from_secs(60));
            let status = client
                .get_agent(&terminal)
                .unwrap()
                .map(|agent| agent.agent_status);
            (heard, Instant::now(), status)
        })
    };
    person.types(
        " the numbers from 1 to 200, one per line, then the word banana, in capitals",
        PACE,
    );
    person.presses_enter();
    let entered = Instant::now();
    drop(person);
    sender.join().unwrap().unwrap();
    let (heard, at, status) = hook.join().unwrap();
    assert_eq!(
        heard,
        Some(Heard::Submitted),
        "Assumption: Claude Code's hook reports the prompt herdr-core typed, without tags, as \
         it queues it while it works. Its terminal shows:\n{}",
        agent.screen()
    );
    // herdr-core 0.1.20 and later type a send held behind the person's typing 0.2 s after their
    // Enter, where 0.1.18 waited its quiet pause of about 3 s.
    assert!(
        at >= entered,
        "Assumption: herdr-core waits until the person sends their typing before it types"
    );
    assert_eq!(
        status,
        Some(AgentStatus::Working),
        "the user's turn ended before herdr-core typed: it no longer tests a queued prompt"
    );
    agent.printed(replies, "Claude Code runs the reviewer's prompt");
    let screen = agent.screen();
    assert!(
        screen.contains("BANANA") && !screen.contains(BLOCKED),
        "Assumption: Claude Code runs the user's prompt, which the hook lets go on. Its \
         terminal shows:\n{screen}"
    );
    agent.wait_for_prompt();
}

fn a_long_prompt_is_heard_as_typed(agent: &RealAgent, hooked: &HookedAgent) {
    let plum = format!(
        "Reply with only the word plum, in capitals.\n{}\n",
        (1..=40)
            .map(|line| format!("Context line {line}, which needs no answer."))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let expectation = agent
        .hooks
        .expect_prompt(hooked, &plum, BLOCKED, agent.others());
    let replies = agent.output("PLUM");
    agent.client.submit_prompt(&agent.terminal, &plum).unwrap();
    assert_eq!(
        expectation.wait(HOOK_LIMIT),
        Some(Heard::Submitted),
        "Assumption: the hook reports a long pasted prompt as typed, once claude-hooks removes \
         its paste tags. Its terminal shows:\n{}",
        agent.screen()
    );
    drop(expectation);
    agent.printed(replies, "Claude Code runs the long prompt");
    agent.wait_for_prompt();
}

fn a_prompt_starts_the_agent(agent: &RealAgent) {
    let replies = agent.output("FIG");
    let started = agent
        .client
        .prompt_agent(&agent.terminal, "Reply with only the word fig, in capitals");
    assert!(
        started.is_ok(),
        "Assumption: Claude Code reports working, with OSC 7501, within herdr-core's window \
         after a send: {started:?}. Its terminal shows:\n{}",
        agent.screen()
    );
    agent.printed(replies, "Claude Code runs the prompt");
    agent.wait_for_prompt();
}

fn nothing_is_typed_while_a_question_waits(agent: &RealAgent) {
    agent
        .client
        .submit_prompt(
            &agent.terminal,
            "Use the AskUserQuestion tool to ask me which fruit I prefer, apple or pear. Do \
             nothing else.",
        )
        .unwrap();
    agent.wait_for(
        "Assumption: Claude Code reports blocked, with OSC 7501, while a question waits on its \
         user",
        |status| status == AgentStatus::Blocked,
    );
    let replies = agent.output("LIME");
    let (waits, sender) = agent.send_telling_waits("Reply with only the word lime, in capitals");
    assert_eq!(
        waits.recv().unwrap(),
        Some(PromptWait::NoInputLine),
        "Assumption: Claude Code's question shows no input line. Its terminal shows:\n{}",
        agent.screen()
    );
    agent.core.hc(&["keys", &agent.terminal.0, "esc"]);
    sender.join().unwrap().unwrap();
    agent.printed(
        replies,
        "Claude Code runs the prompt that waited for the question",
    );
    agent.wait_for_prompt();
}

fn text_left_in_the_box_holds_the_prompt(agent: &RealAgent, hooked: &HookedAgent) {
    let kiwi = "Reply with only the word kiwi, in capitals";
    let expectation = agent
        .hooks
        .expect_prompt(hooked, kiwi, BLOCKED, agent.others());
    let mut person = agent.core.person_at(&agent.terminal);
    person.types("half", PACE);
    let (waits, sender) = agent.send_telling_waits(kiwi);
    assert_eq!(
        waits.recv().unwrap(),
        Some(PromptWait::TypedText),
        "Assumption: the client reads the person's text in Claude Code's input line. Its \
         terminal shows:\n{}",
        agent.screen()
    );
    // herdr-core holds a send only for its quiet pause after the person's last key.
    thread::sleep(Duration::from_secs(6));
    assert!(
        agent.screen().contains("half"),
        "the person's text stays in the box, with nothing typed over it"
    );
    person.erases_line();
    drop(person);
    sender.join().unwrap().unwrap();
    assert_eq!(
        expectation.wait(HOOK_LIMIT),
        Some(Heard::Submitted),
        "Assumption: the prompt goes alone once the box is cleared. Its terminal shows:\n{}",
        agent.screen()
    );
}

struct RealAgent {
    hooks: AgentHooks,
    client: HcClient,
    terminal: PaneId,
    core: PrivateCore,
}

impl RealAgent {
    fn send_telling_waits(
        &self,
        text: &'static str,
    ) -> (
        std::sync::mpsc::Receiver<Option<PromptWait>>,
        thread::JoinHandle<herdr_client::Result<()>>,
    ) {
        let (client, terminal) = (self.client.clone(), self.terminal.clone());
        let (told, waits) = std::sync::mpsc::channel();
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

    fn start() -> Self {
        let core = PrivateCore::start();
        let hooks =
            AgentHooks::listen(&HookDirectory::for_server(core.runtime(), &core.socket())).unwrap();
        let mut argv = vec![
            "bash".to_owned(),
            "-c".to_owned(),
            "read -r; exec \"$@\"".to_owned(),
        ];
        argv.push("bash".to_owned());
        argv.extend(claude_environment());
        let settings = serde_json::json!({
            "pluginConfigs": {
                PLUGIN: {"options": {"control": env!("CARGO_BIN_EXE_reviewer-control")}},
            },
        })
        .to_string();
        argv.extend(["claude", "--model", MODEL, "--settings", &settings].map(String::from));
        let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
        let terminal = core.start_in(&folder(), &argv);
        let agent = Self {
            hooks,
            client: core.client(),
            terminal,
            core,
        };
        // Claude Code draws the spaces between words with cursor moves, which `hc wait` strips.
        let question = agent.output("trust");
        agent.core.hc(&["keys", &agent.terminal.0, "enter"]);
        agent.trust_the_folder(question);
        agent.wait_for_prompt();
        agent
    }

    fn trust_the_folder(&self, question: Child) {
        let (outcome, heard) = std::sync::mpsc::channel();
        let asked = outcome.clone();
        let (client, terminal) = (self.client.clone(), self.terminal.clone());
        let canceller = EventCanceller::default();
        let started = canceller.clone();
        thread::spawn(move || {
            let _ = client.forward_agent_status(&terminal, &started, &mut |status| {
                !status.waits_for_prompt()
            });
            let _ = outcome.send(false);
        });
        let asking = thread::spawn(move || {
            let ended = question.wait_with_output();
            let _ = asked.send(true);
            ended
        });
        if heard.recv().unwrap() {
            let ended = asking.join().unwrap().unwrap();
            assert!(
                String::from_utf8_lossy(&ended.stdout).contains("\"output\""),
                "Claude Code did not start: {ended:?}. Its terminal shows:\n{}",
                self.screen()
            );
            self.choose_yes();
        }
        canceller.cancel();
    }

    // Claude Code drops the keys pressed before it takes input, and nothing tells when it does.
    fn choose_yes(&self) {
        for _ in 0..20 {
            self.core.hc(&["keys", &self.terminal.0, "down"]);
            self.core.hc(&[
                "wait",
                &self.terminal.0,
                "--output",
                "trust",
                "--timeout",
                "1",
                "--json",
            ]);
            if self.screen().contains("❯ Yes") {
                self.core.hc(&["keys", &self.terminal.0, "enter"]);
                return;
            }
        }
        panic!(
            "Claude Code's question about the folder took no Down. Its terminal shows:\n{}",
            self.screen()
        );
    }

    fn others(&self) -> OtherPrompts {
        OtherPrompts::for_host(&self.client)
    }

    // Claude Code's session hook may end after it took its first prompt, and nothing announces the
    // session it records.
    fn hooked(&self) -> HookedAgent {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let agent = self.client.get_agent(&self.terminal).unwrap();
            if let Some(hooked) = agent.and_then(|agent| self.hooks.hooked(&self.client, &agent)) {
                return hooked;
            }
            assert!(
                Instant::now() < deadline,
                "Assumption: the reviewer's hook names Claude Code's session in a label of its \
                 terminal, as the agent that leads its foreground, and records it. Is the \
                 reviewer's plugin installed (make install)? Its terminal shows:\n{}",
                self.screen()
            );
            thread::sleep(Duration::from_millis(100));
        }
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
            "Assumption: {assumption}: {ended:?}. Its terminal shows:\n{}",
            self.screen()
        );
    }

    fn wait_for_prompt(&self) {
        self.wait_for(
            "Claude Code ended its turn, or started",
            AgentStatus::waits_for_prompt,
        );
    }

    fn wait_for(&self, assumption: &str, reached: impl Fn(AgentStatus) -> bool) {
        let mut last = None;
        let _ = self.client.forward_agent_status(
            &self.terminal,
            &EventCanceller::default(),
            &mut |status| {
                last = Some(status);
                !reached(status)
            },
        );
        assert!(
            last.is_some_and(&reached),
            "{assumption}: it reports {last:?}. Its terminal shows:\n{}",
            self.screen()
        );
    }

    fn screen(&self) -> String {
        self.core.screen(&self.terminal)
    }
}

// Claude Code 2.1.318 copies itself into `$XDG_DATA_HOME/claude/versions` and points
// `~/.local/bin/claude` there, even with its updater off.
fn claude_environment() -> Vec<String> {
    let mut unset = Vec::new();
    let mut set = vec!["DISABLE_AUTOUPDATER=1".to_owned()];
    for (name, _) in std::env::vars_os() {
        if let Some(name) = name.to_str()
            && (name.starts_with("CLAUDE") || name.starts_with("ANTHROPIC"))
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

// Claude Code asks whether to trust a folder until the owner's configuration records it.
fn folder() -> PathBuf {
    let cache = std::env::var_os("XDG_CACHE_HOME").map_or_else(
        || PathBuf::from(std::env::var_os("HOME").unwrap()).join(".cache"),
        PathBuf::from,
    );
    let folder = cache.join("herdr-reviewer").join("hc-contract-check");
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).unwrap();
    folder
}
