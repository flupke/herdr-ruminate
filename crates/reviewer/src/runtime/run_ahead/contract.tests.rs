use std::ffi::OsString;
use std::process::Command;

use review_explore::{AnswerInput, ExploreRound, Question, TurnRequest};
use review_run_ahead::ForkRecord;

use super::*;

const REAL_MODEL: &str = "claude-sonnet-5-5";
const REAL_TURN: Duration = Duration::from_secs(900);

const REAL_CHANGE: &[u8] =
    b"/// What a resolved review conversation does when the code it is attached to changes.\n\
      pub enum Reopen {\n    Never,\n    WhenItsLinesChange,\n    WhenItsFileChanges,\n}\n\n\
      /// The policy every review applies; there is no setting for it.\n\
      pub const POLICY: Reopen = Reopen::WhenItsFileChanges;\n\n\
      /// Whether a resolved conversation reopens under `policy`.\n\
      pub fn reopens(policy: &Reopen, lines_changed: bool, file_changed: bool) -> bool {\n    \
      match policy {\n        Reopen::Never => false,\n        \
      Reopen::WhenItsLinesChange => lines_changed,\n        \
      Reopen::WhenItsFileChanges => file_changed,\n    }\n}\n";

#[derive(Clone, Copy, Debug)]
enum Assumption {
    Starts,
    ForkCommandLine,
    ResumeFollowed,
    TypedInOrder,
    Transcripts,
}

impl Assumption {
    fn says(self) -> &'static str {
        match self {
            Self::Starts => {
                "Claude Code starts in the pane, logged in, with Herdr's and the reviewer's \
                 hooks, and takes an Explore turn through the reviewer's tools"
            }
            Self::ForkCommandLine => {
                "a fork started with the command line run-ahead builds takes its turn and \
                 submits it with its own access"
            }
            Self::ResumeFollowed => {
                "a typed /resume moves the agent to the session, and Herdr reports it there"
            }
            Self::TypedInOrder => {
                "Claude Code takes what is typed in its pane in order, so what follows a \
                 /resume reaches that session"
            }
            Self::Transcripts => {
                "Claude Code keeps each session's transcript under projects/ in its \
                 configuration directory, where run-ahead deletes a fork's"
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Agent {
    Fake,
    Real,
}

struct ContractRun {
    run: RunAheadFlow,
    agent: Agent,
    session: String,
    resting_on: Assumption,
}

impl ContractRun {
    fn start(agent: Agent, waits: review_run_ahead::ForkWaits) -> Self {
        let (run, session) = match agent {
            Agent::Fake => (
                RunAheadFlow::start_with(FakeAgent::Forkable, waits),
                "session".into(),
            ),
            Agent::Real => {
                let session = uuid::Uuid::new_v4().to_string();
                let start = std::panic::AssertUnwindSafe(|| real_run(&session, waits));
                let run = std::panic::catch_unwind(start).unwrap_or_else(|panic| {
                    eprintln!("{}", failed(Agent::Real, Assumption::Starts));
                    std::panic::resume_unwind(panic)
                });
                (run, session)
            }
        };
        Self {
            run,
            agent,
            session,
            resting_on: Assumption::Starts,
        }
    }

    fn rests_on(&mut self, assumption: Assumption) {
        self.resting_on = assumption;
    }

    fn patience(&self) -> Duration {
        match self.agent {
            Agent::Fake => GUARD,
            Agent::Real => REAL_TURN,
        }
    }

    fn herdr(&self) -> &IsolatedHerdrServer {
        self.run.herdr()
    }

    fn kickoff(&mut self) -> Question {
        let flow = &mut self.run.flow;
        let kickoff = flow.exploration.request(None, None).unwrap();
        let kickoff = flow.post_turn(&kickoff);
        flow.wait_for_prompt(&kickoff);
        self.agent_turn(&kickoff, 1);
        self.run
            .flow
            .exploration
            .questions
            .last()
            .cloned()
            .expect("the agent asked a question")
    }

    fn agent_turn(&mut self, request: &TurnRequest, version: u32) {
        let patience = self.patience();
        let statuses = self.herdr().server.agent_statuses(&self.herdr().pane_id);
        // Herdr may still report the agent idle as it reads the prompt.
        statuses.wait_within(patience, herdr_client::protocol::AgentStatus::Working);
        let flow = &mut self.run.flow;
        match self.agent {
            Agent::Fake => {
                let accepted =
                    flow.submit(&super::super::explore_flow::agent_update(request, version));
                assert_ne!(accepted.is_error, Some(true), "{accepted:?}");
                flow.fixture
                    .herdr
                    .events()
                    .send(&agent_role(), &fake_agent::Command::EndTurn);
            }
            Agent::Real => loop {
                let event = flow
                    .fixture
                    .runtime
                    .recv_timeout(patience)
                    .expect("the agent's turn did not end");
                if let Some(committed) = event.downcast_ref::<ui_events::ExploreCommitted>() {
                    flow.exploration = committed.round.exploration.clone();
                    let _ = committed.response.send(Ok(committed.applied));
                    if committed
                        .round
                        .exploration
                        .conversation
                        .iter()
                        .any(|entry| entry.update.request == request.request)
                    {
                        break;
                    }
                }
            },
        }
        statuses.wait_within(patience, herdr_client::protocol::AgentStatus::Idle);
    }

    fn forks_take_turns(&mut self, count: usize) -> RoundForks {
        if self.agent == Agent::Fake {
            for session in self.run.wait_for_forks(count) {
                self.run.fork_submits(&session);
                self.herdr()
                    .events()
                    .send(&Role::fork(session), &fake_agent::Command::EndTurn);
            }
        }
        let saved = self.wait_for_saved("the forks' end", |saved| {
            (saved.forks.len() == count && saved.forks.iter().all(|fork| fork.exit.is_some()))
                .then(|| saved.clone())
        });
        eprintln!("run-ahead's log:\n{}", self.run.log());
        saved
    }

    fn fork_with_a_turn(saved: &RoundForks) -> ForkRecord {
        saved
            .forks
            .iter()
            .find(|fork| fork.turn.as_ref().is_some_and(|turn| turn.next.is_some()))
            .or_else(|| saved.forks.iter().find(|fork| fork.turn.is_some()))
            .expect("a fork prepared a turn")
            .clone()
    }

    fn wait_for_saved<T>(&self, what: &str, check: impl Fn(&RoundForks) -> Option<T>) -> T {
        self.run.flow.state.wait_within(
            self.patience(),
            what,
            || check(&self.run.saved()),
            || format!("run-ahead's log:\n{}", self.run.log()),
        )
    }

    fn wait_for_turns(&self, count: usize) -> ExploreRound {
        let flow = &self.run.flow;
        flow.state.wait_within(
            self.patience(),
            &format!("{count} turns"),
            || {
                let saved = flow.saved();
                (saved.exploration.conversation.len() >= count).then_some(saved)
            },
            || format!("run-ahead's log:\n{}", self.run.log()),
        )
    }

    fn resumed(&self, count: usize) -> Vec<String> {
        self.herdr()
            .events()
            .wait_until(&format!("{count} resumed sessions"), |received| {
                resumed_sessions(received).len() >= count
            });
        self.herdr().resumed()
    }

    fn reported_session(&self) -> Option<String> {
        self.herdr()
            .client()
            .get_agent(&self.herdr().pane_id)
            .unwrap()
            .and_then(|agent| agent.agent_session)
            .map(|session| session.value)
    }

    fn transcripts(&self) -> claude_fork::Transcripts {
        claude_fork::Transcripts::at(self.run.root().join(CLAUDE_CONFIG).join("projects"))
    }

    fn reached(&self, session: &str, request: &TurnRequest) -> bool {
        let explore_request = format!("Explore request: {}\n", request.request);
        let prompts = self.herdr().prompts();
        let prompt = prompts
            .iter()
            .find(|prompt| prompt.contains(&explore_request))
            .expect("the agent read the prompt");
        let index = prompt
            .split_whitespace()
            .find(|word| word.ends_with("/index.md"))
            .expect("the prompt names its turn's index");
        let path = self
            .transcripts()
            .find(session)
            .unwrap_or_else(|| panic!("no transcript of the session {session}"));
        fs::read_to_string(path).unwrap().contains(index)
    }
}

impl Drop for ContractRun {
    fn drop(&mut self) {
        if !thread::panicking() {
            return;
        }
        eprintln!("{}", failed(self.agent, self.resting_on));
        if self.agent == Agent::Real {
            eprintln!("Its pane shows:\n{}", self.herdr().screen());
        }
    }
}

fn failed(agent: Agent, assumption: Assumption) -> String {
    let assumption = assumption.says();
    match agent {
        Agent::Fake => format!(
            "\nThe rehearsal failed where the test checks that {assumption}: the fake agent plays \
             it, so the fault is the test's."
        ),
        Agent::Real => format!(
            "\nThe contract check failed where the test checks that {assumption}: real Claude \
             Code may no longer behave as the fake agent assumes."
        ),
    }
}

fn continue_as_the_fork_of_a_bare_answer(mut contract: ContractRun) {
    let question = contract.kickoff();

    contract.rests_on(Assumption::ForkCommandLine);
    let saved = contract.forks_take_turns(question.alternatives.len());
    contract.check_prepared_turns(&saved);
    let fork = ContractRun::fork_with_a_turn(&saved);

    contract.rests_on(Assumption::ResumeFollowed);
    contract.stop_running_ahead();
    let request = contract.run.answer_bare(&fork.choice);
    let round = contract.wait_for_turns(2);
    let turn = &round.exploration.conversation.last().unwrap().update;
    assert_eq!(
        turn.request, request.request,
        "the fork's turn is the round's"
    );
    assert_eq!(contract.resumed(1), std::slice::from_ref(&fork.session));
    assert_eq!(
        contract.reported_session().as_ref(),
        Some(&fork.session),
        "Herdr reports the agent on the fork's session"
    );

    contract.rests_on(Assumption::Transcripts);
    contract.check_only_the_fork_s_transcript_stays(&fork);
    if turn.conclusion.is_some() {
        eprintln!("the fork concluded the round: no next answer to send");
        return;
    }

    contract.rests_on(Assumption::TypedInOrder);
    contract.run.flow.exploration = round.exploration.clone();
    let next = contract.answer_with_a_comment();
    assert!(
        contract.reached(&fork.session, &next),
        "the answer reached the fork's session"
    );
    let round = contract.wait_for_turns(3);
    assert_eq!(
        round
            .exploration
            .conversation
            .last()
            .unwrap()
            .update
            .request,
        next.request,
        "the agent's turn on the fork's session is accepted"
    );
    let agent = contract
        .herdr()
        .client()
        .get_agent(&contract.herdr().pane_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        round.last_agent_session,
        review_explore::ConversationBinding::from_agent(&agent)
    );
}

impl ContractRun {
    fn check_prepared_turns(&self, saved: &RoundForks) {
        let prepared: Vec<_> = saved
            .forks
            .iter()
            .filter_map(|fork| fork.turn.as_ref())
            .collect();
        assert!(!prepared.is_empty(), "no fork prepared a turn");
        let requests: std::collections::HashSet<_> =
            prepared.iter().map(|turn| &turn.request).collect();
        assert_eq!(
            requests.len(),
            prepared.len(),
            "each fork submits its own request"
        );
        let instance = &self.run.flow.exploration.instance;
        assert!(prepared.iter().all(|turn| turn.instance == *instance));
        assert!(
            saved
                .forks
                .iter()
                .all(|fork| fork.usage.is_some_and(|usage| usage.output > 0)),
            "each fork's output says what it spent"
        );
        assert_eq!(
            self.run.flow.saved().exploration.questions.len(),
            1,
            "a fork adds no question to the round"
        );
    }

    fn stop_running_ahead(&self) {
        self.run
            .flow
            .fixture
            .runtime
            .store
            .save_explore_run_ahead(RunAhead::Off)
            .unwrap();
    }

    fn check_only_the_fork_s_transcript_stays(&self, fork: &ForkRecord) {
        self.wait_for_saved("the other forks' clean-up", |saved| {
            saved
                .forks
                .iter()
                .filter(|other| other.session != fork.session)
                .all(|other| other.cleaned)
                .then_some(())
        });
        for other in self.run.saved().forks {
            if other.session == fork.session {
                assert!(!other.cleaned, "the fork's session is the agent's now");
                assert!(self.transcripts().find(&fork.session).is_some());
            } else {
                assert!(
                    !claude_fork::process(&other.host).unwrap().is_running(),
                    "a fork outlived its question"
                );
                assert_eq!(self.transcripts().find(&other.session), None);
            }
        }
    }

    fn answer_with_a_comment(&mut self) -> TurnRequest {
        let flow = &mut self.run.flow;
        let question = flow.exploration.questions.last().cloned().unwrap();
        let answer = AnswerInput {
            option: Some(question.alternatives[0].id.clone()),
            text: "Yes, and write down why.".into(),
            in_reply_to: None,
            first_pick: None,
            ..AnswerInput::default()
        };
        let request = flow
            .exploration
            .request(Some(answer), Some(&question))
            .unwrap();
        let request = flow.post_turn(&request);
        flow.wait_for_prompt(&request);
        self.agent_turn(&request, 3);
        request
    }
}

fn settle_back_on_the_agent_s_own_session(mut contract: ContractRun) {
    let question = contract.kickoff();

    contract.rests_on(Assumption::ForkCommandLine);
    let saved = contract.forks_take_turns(question.alternatives.len());
    let fork = ContractRun::fork_with_a_turn(&saved);

    contract.rests_on(Assumption::ResumeFollowed);
    if contract.agent == Agent::Fake {
        // Real Claude Code's hook reports a resume about 0.5 s after it is typed.
        fs::write(contract.herdr().directory().unreported_resume(), "").unwrap();
    }
    contract.stop_running_ahead();
    let request = contract.run.answer_bare(&fork.choice);
    contract.wait_for_saved("the switch undone", |saved| {
        let continued = &saved.fork(&fork.session)?.continued;
        matches!(continued, Some(Continuation::Undone { .. })).then_some(())
    });
    let resumed = contract.resumed(2);

    contract.rests_on(Assumption::TypedInOrder);
    assert_eq!(resumed, [fork.session.clone(), contract.session.clone()]);
    assert_eq!(
        contract.reported_session().as_ref(),
        Some(&contract.session),
        "Herdr reports the agent back on its own session"
    );
    assert_eq!(contract.run.flow.saved().exploration.conversation.len(), 1);

    contract
        .run
        .flow
        .fixture
        .explore(ExploreCommand::Retry(Box::new(request.clone())));
    contract.run.flow.wait_for_prompt(&request);
    contract.agent_turn(&request, 2);
    assert!(
        contract.reached(&contract.session, &request),
        "the answer reached the agent's own session"
    );
    let explore_request = format!("Explore request: {}", request.request);
    assert_eq!(
        contract
            .herdr()
            .prompts()
            .iter()
            .filter(|prompt| prompt.contains(&explore_request))
            .count(),
        1,
        "the answer reached the agent once"
    );
}

#[test]
fn rehearse_continuing_as_the_fork_of_a_bare_answer() {
    continue_as_the_fork_of_a_bare_answer(ContractRun::start(Agent::Fake, test_waits()));
}

#[test]
#[ignore = "the run-ahead contract check: real Claude Code turns on the token's account; see the real-agent-checks skill"]
fn real_claude_code_continues_as_the_fork_of_a_bare_answer() {
    continue_as_the_fork_of_a_bare_answer(ContractRun::start(Agent::Real, test_waits()));
}

fn unconfirmed_switch() -> review_run_ahead::ForkWaits {
    review_run_ahead::ForkWaits {
        resume: Duration::ZERO,
        ..test_waits()
    }
}

#[test]
fn rehearse_settling_back_on_the_agent_s_own_session() {
    settle_back_on_the_agent_s_own_session(ContractRun::start(Agent::Fake, unconfirmed_switch()));
}

#[test]
#[ignore = "the run-ahead contract check: real Claude Code turns on the token's account; see the real-agent-checks skill"]
fn real_claude_code_settles_back_on_its_own_session() {
    settle_back_on_the_agent_s_own_session(ContractRun::start(Agent::Real, unconfirmed_switch()));
}

const TOKEN_VARIABLE: &str = "CLAUDE_CODE_OAUTH_TOKEN";

fn real_run(session: &str, waits: review_run_ahead::ForkWaits) -> RunAheadFlow {
    assert!(
        std::env::var_os(TOKEN_VARIABLE).is_some(),
        "set {TOKEN_VARIABLE} to a token of `claude setup-token`: the contract check logs in \
         with it, and never with the owner's Claude Code configuration"
    );
    let repository_files = repository_fixture(RepoType::Git);
    repository_files.write("reviewed.rs", REAL_CHANGE);
    let repository = repository_files.root().to_owned();
    let mut herdr = IsolatedHerdrServer::create(&repository, "claude", |root| {
        real_variables(root)
            .into_iter()
            .map(|(name, value)| format!("{name}={}", value.to_string_lossy()))
            .collect()
    });
    herdr.real_agent = true;
    let configuration = RealConfiguration::write(&herdr, &repository);
    let fixture = ReviewFlowFixture::start_on(repository_files, herdr, |setup, _| {
        setup.run_ahead.waits = waits;
    });
    configuration.serve_reviewer(fixture.endpoint.address().port());
    let herdr = &fixture.herdr;
    let mut command = vec!["pane", "run", &herdr.pane_id.0, "env"];
    let owner_s = owner_s_claude_variables();
    for name in &owner_s {
        command.extend(["-u", name]);
    }
    command.extend(["claude", "--session-id", session, "--model", REAL_MODEL]);
    herdr.run_cli(&command);
    herdr.events().wait_for("the agent's session", |reported| {
        reported.event
            == fake_agent::Event::SessionReported {
                session: session.into(),
                start: "startup".into(),
            }
    });
    // Herdr hears of the session from its own hook, and sends no event once it does.
    let deadline = std::time::Instant::now() + GUARD;
    loop {
        let agent = herdr.client().get_agent(&herdr.pane_id).unwrap();
        if agent.as_ref().is_some_and(|agent| {
            agent.agent_status.waits_for_prompt()
                && agent
                    .agent_session
                    .as_ref()
                    .is_some_and(|reported| reported.value == session)
        }) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Herdr does not report the agent idle on its session: {agent:?}; its pane shows:\n{}",
            herdr.screen()
        );
        thread::sleep(Duration::from_millis(100));
    }
    fixture
        .runtime
        .store
        .save_explore_run_ahead(RunAhead::Every)
        .unwrap();
    RunAheadFlow {
        flow: ExploreFlow::start_on(fixture),
    }
}

fn real_variables(root: &Path) -> [(&'static str, OsString); 3] {
    [
        ("CLAUDE_CONFIG_DIR", root.join(CLAUDE_CONFIG).into()),
        // Claude Code keeps its MCP servers' logs there, outside its configuration.
        ("XDG_CACHE_HOME", root.join("cache").into()),
        ("DISABLE_AUTOUPDATER", "1".into()),
    ]
}

// The Claude Code variables of a session the check runs under would load its plugins from
// `~/.claude` and reach that session.
fn owner_s_claude_variables() -> Vec<String> {
    std::env::vars_os()
        .filter_map(|(name, _)| name.into_string().ok())
        .filter(|name| name.starts_with("CLAUDE") || name.starts_with("ANTHROPIC"))
        .filter(|name| name != TOKEN_VARIABLE && name != "CLAUDE_CONFIG_DIR")
        .collect()
}

impl IsolatedHerdrServer {
    fn screen(&self) -> String {
        Command::new(self.server.binary())
            .args(["pane", "read", &self.pane_id.0, "--source", "visible"])
            .env_clear()
            .envs(self.server.environment())
            .output()
            .map_or_else(
                |error| format!("(unread: {error})"),
                |read| String::from_utf8_lossy(&read.stdout).into_owned(),
            )
    }
}

struct RealConfiguration(PathBuf);

impl RealConfiguration {
    fn write(herdr: &IsolatedHerdrServer, repository: &Path) -> Self {
        let root = herdr.server.root().join(CLAUDE_CONFIG);
        fs::create_dir_all(&root).unwrap();
        let configuration = Self(root);
        Self::run(
            herdr,
            herdr.server.binary(),
            &["integration", "install", "claude"],
        );
        let plugin = Path::new(env!("CARGO_MANIFEST_DIR")).join("../claude-hooks");
        Self::run(
            herdr,
            Path::new("claude"),
            &["plugin", "marketplace", "add", &plugin.to_string_lossy()],
        );
        let control = effects::fixture::test_fork_tools().control;
        Self::run(
            herdr,
            Path::new("claude"),
            &[
                "plugin",
                "install",
                "progressive-reviewer@herdr-progressive-reviewer",
                "--config",
                &format!("control={}", control.display()),
            ],
        );
        configuration.edit("settings.json", |settings| {
            let observing = fake_agent::observing_hooks(&fake_agent_program());
            for (event, hooks) in observing.as_object().unwrap() {
                let entries = settings["hooks"][event].as_array().cloned();
                let mut entries = entries.unwrap_or_default();
                entries.extend(hooks.as_array().unwrap().iter().cloned());
                settings["hooks"][event] = entries.into();
            }
            settings["permissions"] = serde_json::json!({
                "defaultMode": "dontAsk",
                "allow": claude_fork::SUBMITS,
                "additionalDirectories": [std::env::temp_dir()],
            });
        });
        configuration.edit(".claude.json", |state| {
            state["hasCompletedOnboarding"] = true.into();
            state["theme"] = "dark".into();
            // Claude Code 2.1.310 offers to make auto mode the default at the first prompt
            // typed with "don't ask" on, and its dialog takes that prompt.
            state["hasSeenAutoDefaultNudge"] = true.into();
            state["hasSeenAutoModeEntryWarning"] = true.into();
            for path in [repository.to_owned(), repository.canonicalize().unwrap()] {
                state["projects"][path.to_string_lossy().as_ref()] = serde_json::json!({
                    "hasTrustDialogAccepted": true,
                    "hasCompletedProjectOnboarding": true,
                });
            }
        });
        configuration
    }

    fn serve_reviewer(&self, port: u16) {
        let bridge = effects::fixture::test_fork_tools()
            .control
            .with_file_name("reviewer-mcp");
        assert!(
            bridge.is_file(),
            "{} is not built: see the real-agent-checks skill",
            bridge.display()
        );
        self.edit(".claude.json", |state| {
            state["mcpServers"]["herdr_reviewer"] = serde_json::json!({
                "type": "stdio",
                "command": bridge,
                "args": [port.to_string()],
            });
        });
    }

    fn run(herdr: &IsolatedHerdrServer, program: &Path, arguments: &[&str]) {
        let owner_s = owner_s_claude_variables();
        let output = Command::new(program)
            .args(arguments)
            .current_dir(herdr.server.root())
            .env_clear()
            .envs(herdr.server.environment().iter().filter(|(name, _)| {
                !owner_s
                    .iter()
                    .any(|owner_s| name.as_os_str() == owner_s.as_str())
            }))
            .envs(real_variables(herdr.server.root()))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{} {} failed:\n{}{}",
            program.display(),
            arguments.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn edit(&self, name: &str, change: impl FnOnce(&mut serde_json::Value)) {
        let path = self.0.join(name);
        let mut value = fs::read(&path).map_or_else(
            |_| serde_json::json!({}),
            |content| serde_json::from_slice(&content).unwrap(),
        );
        change(&mut value);
        fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    }
}
