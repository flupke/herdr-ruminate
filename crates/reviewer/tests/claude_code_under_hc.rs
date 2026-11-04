//! The contract check of prompt delivery under herdr-core: the owner's Claude Code, in a terminal
//! of a private herdr-core, takes the reviewer's prompts as the reviewer expects. Each step names
//! the assumption it rests on. Opt-in: real Claude Code turns on the owner's login, with the
//! reviewer's plugin installed, and herdr-core, which the dev shell does not pin
//! (`.agents/wiki/herdr-app.md`, "The contract check").

use std::fs;
use std::path::PathBuf;
use std::process::{Child, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use agent_hooks::{AgentHooks, Heard, HookDirectory, HookedAgent, OtherPrompts};
use hc_client::HcClient;
use hc_test_support::PrivateCore;
use herdr_client::client::EventCanceller;
use herdr_client::protocol::{AgentHost, AgentPort, AgentStatus, PaneId};

/// The reviewer's plugin, as `make install` installs it in the owner's Claude Code.
const PLUGIN: &str = "progressive-reviewer@herdr-progressive-reviewer";
const MODEL: &str = "claude-sonnet-5-5";
/// How long the reviewer waits for its hook to report a prompt once herdr-core typed it: the
/// `HOOK_LIMIT` of `review-thread-service`'s delivery.
const HOOK_LIMIT: Duration = Duration::from_secs(5);
const BLOCKED: &str = "The progressive reviewer's contract check stopped this prompt.";
/// How fast the person types.
const PACE: Duration = Duration::from_millis(150);

#[test]
#[ignore = "the contract check under herdr-core: real Claude Code turns on the owner's login; see .agents/wiki/herdr-app.md"]
fn claude_code_takes_the_reviewer_s_prompts_through_herdr_core() {
    let agent = RealAgent::start();
    let hooked = agent.hooked();

    a_prompt_starts_the_agent(&agent);
    a_long_prompt_is_heard_as_typed(&agent, &hooked);
    nothing_is_typed_while_a_question_waits(&agent);
    // herdr-core refuses to type in a terminal younger than its startup age limit until its
    // output settles, which the person's keys keep from happening: the turns before outlast it.
    the_user_s_prompt_goes_on_while_the_reviewer_s_waits(&agent, &hooked);
    text_left_in_the_box_joins_the_prompt(&agent, &hooked);
}

/// The user types a prompt of their own while the reviewer's waits for them to stop, and the
/// reviewer's comes while Claude Code works on the user's, which it queues.
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
    assert!(
        at >= entered + Duration::from_secs(2),
        "Assumption: herdr-core waits until the person stops typing before it types"
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

/// A long prompt of several lines, as an Explore dispatch is, which Claude Code reports as a
/// paste, with the line break the wakeups end with.
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

/// An agent without the reviewer's hooks counts as started on a prompt by its status.
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

/// A question that waits on the user blocks the agent, and herdr-core types nothing then.
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
    let refused = agent
        .client
        .submit_prompt(&agent.terminal, "Reply with only the word lime");
    assert!(
        matches!(&refused, Err(herdr_client::Error::NotTyped { reason }) if reason.contains("waits on its user")),
        "{refused:?}"
    );
    agent.core.hc(&["keys", &agent.terminal.0, "esc"]);
    agent.wait_for_prompt();
}

/// Text left in the input box joins the reviewer's prompt, which the hook blocks.
fn text_left_in_the_box_joins_the_prompt(agent: &RealAgent, hooked: &HookedAgent) {
    let kiwi = "Reply with only the word kiwi, in capitals";
    let expectation = agent
        .hooks
        .expect_prompt(hooked, kiwi, BLOCKED, agent.others());
    let mut person = agent.core.person_at(&agent.terminal);
    person.types("half", PACE);
    drop(person);
    agent.client.submit_prompt(&agent.terminal, kiwi).unwrap();
    assert_eq!(
        expectation.wait(HOOK_LIMIT),
        Some(Heard::Joined {
            left: "half".into()
        }),
        "Assumption: Claude Code submits the text left in its input box and the prompt herdr-core \
         typed as one. Its terminal shows:\n{}",
        agent.screen()
    );
}

/// The owner's Claude Code in a terminal of a private herdr-core, with the reviewer's plugin
/// running this build's `reviewer-control`, and the reviewer's end of its hooks.
struct RealAgent {
    hooks: AgentHooks,
    client: HcClient,
    terminal: PaneId,
    core: PrivateCore,
}

impl RealAgent {
    /// Starts the owner's Claude Code, on their own login and configuration, in [`folder`].
    fn start() -> Self {
        let core = PrivateCore::start();
        let hooks =
            AgentHooks::listen(&HookDirectory::for_server(core.runtime(), &core.socket())).unwrap();
        // The terminal waits for a line before it starts Claude Code, so that the wait for its
        // question about the folder begins well before Claude Code draws it.
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

    /// Answers yes when Claude Code asks whether to trust the folder, which it asks until the
    /// owner's configuration records it, once: `question` waits for it.
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

    /// Moves the dialog's choice to yes, and takes it. Claude Code drops the keys pressed
    /// before it takes input, which nothing tells: each Down goes again, once the dialog drew
    /// again, until it shows the choice on yes.
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

    /// What delivery does with the other prompts while it types one through herdr-core.
    fn others(&self) -> OtherPrompts {
        OtherPrompts::for_host(&self.client)
    }

    /// The agent as its hooks name it, once its hook named its session. The hook runs as
    /// Claude Code starts, and may end after it took its first prompt; the session it records
    /// for the reviewer, nothing announces: this asks again until it is there.
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

    /// Waits for what the agent prints next that holds `text`: start it before what prints.
    fn output(&self, text: &str) -> Child {
        self.core
            .command(&["wait", &self.terminal.0, "--output", text, "--json"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap()
    }

    /// Waits until the agent printed what `output` waits for.
    fn printed(&self, output: Child, assumption: &str) {
        let ended = output.wait_with_output().unwrap();
        assert!(
            String::from_utf8_lossy(&ended.stdout).contains("\"output\""),
            "Assumption: {assumption}: {ended:?}. Its terminal shows:\n{}",
            self.screen()
        );
    }

    /// Waits until the agent waits for a prompt.
    fn wait_for_prompt(&self) {
        self.wait_for(
            "Claude Code ended its turn, or started",
            AgentStatus::waits_for_prompt,
        );
    }

    /// Waits until the agent reports a status `reached` accepts; fails with `assumption` if its
    /// program ends first.
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

/// An `env` command line that gives the program after it the owner's data, state and
/// configuration folders in place of the core's scratch ones, and none of the Claude Code
/// variables of the test's environment, which the core passes on to its terminals. Claude Code
/// 2.1.318 copies itself into `$XDG_DATA_HOME/claude/versions` and points the owner's
/// `~/.local/bin/claude` there, even with its updater off: in the core's scratch folder, it
/// would leave the owner a `claude` that is gone with the test; its state and configuration are
/// the owner's too. Those of a Claude Code session the check runs under would reach that
/// session.
fn claude_environment() -> Vec<String> {
    // `env` takes its options before the variables it sets.
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

/// The folder the agent works in, emptied: always the same one, under the owner's cache, so that
/// their Claude Code records its trust once.
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
