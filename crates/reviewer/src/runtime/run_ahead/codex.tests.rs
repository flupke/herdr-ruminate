use std::collections::HashSet;
use std::ffi::OsStr;

use codex_daemon::Daemon;
use review_test_support::TestPort;
use review_test_support::codex::{AppServer, CodexHome, ExplorePrompt, Provider};

use super::*;
use crate::runtime::tests::explore_flow::agent_update;

struct CodexRunAhead {
    run: RunAheadFlow,
    provider: Provider,
    unread: Vec<ExplorePrompt>,
    fork_accesses: HashSet<String>,
    _daemon: Option<AppServer>,
    _codex: CodexHome,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Tui {
    OnDaemon,
    WithoutDaemon,
}

impl CodexRunAhead {
    fn start(tui: Tui) -> Self {
        Self::start_with(tui, test_waits())
    }

    fn start_with(tui: Tui, waits: review_run_ahead::ForkWaits) -> Self {
        let repository_files = repository_fixture(RepoType::Git);
        repository_files.write("reviewed.rs", b"pub fn reviewed() {}\n");
        let codex = CodexHome::in_repository(repository_files.root());
        let port = TestPort::new();
        let endpoint =
            review_mcp::Endpoint::for_repository(repository_files.root(), Some(port.number()))
                .unwrap();
        let provider = Provider::start(
            &codex,
            &format!(
                "[mcp_servers.herdr_reviewer]\nurl = \"{}\"\n\
                 [mcp_servers.herdr_reviewer.tools.submit_question]\napproval_mode = \"approve\"\n\
                 [mcp_servers.herdr_reviewer.tools.submit_conclusion]\napproval_mode = \"approve\"\n",
                endpoint.url()
            ),
        );
        let mut daemon = None;
        let herdr =
            IsolatedHerdrServer::start_real(repository_files.root(), "codex", |server, pane| {
                codex.install_herdr_integration(server);
                install_plugin(&codex);
                codex.trust_hooks();
                if tui == Tui::OnDaemon {
                    let variable = |name: &str| server.environment()[OsStr::new(name)].clone();
                    daemon = Some(AppServer::start(
                        &codex,
                        &provider,
                        &[
                            ("XDG_RUNTIME_DIR", &variable("XDG_RUNTIME_DIR")),
                            ("HERDR_SOCKET_PATH", &variable("HERDR_SOCKET_PATH")),
                            ("HERDR_ENV", OsStr::new("1")),
                            ("HERDR_PANE_ID", OsStr::new(&pane.0)),
                            ("SHELL", OsStr::new("/bin/sh")),
                        ],
                    ));
                }
            });
        let fixture =
            ReviewFlowFixture::start_on_port(repository_files, herdr, port, |setup, _| {
                setup.run_ahead.waits = waits;
            });
        let arguments: &[&str] = match tui {
            Tui::OnDaemon => &["--remote", "unix://"],
            Tui::WithoutDaemon => &["--no-daemon"],
        };
        codex.run_tui(&fixture.herdr.server, &fixture.herdr.pane_id, arguments);
        fixture
            .runtime
            .store
            .save_explore_run_ahead(RunAhead::Every)
            .unwrap();
        Self {
            run: RunAheadFlow {
                flow: ExploreFlow::start_on(fixture),
            },
            provider,
            unread: Vec::new(),
            fork_accesses: HashSet::new(),
            _daemon: daemon,
            _codex: codex,
        }
    }

    fn flow(&mut self) -> &mut ExploreFlow {
        &mut self.run.flow
    }

    fn herdr(&self) -> &IsolatedHerdrServer {
        self.run.herdr()
    }

    fn prompt_where(&mut self, wanted: impl Fn(&ExplorePrompt) -> bool) -> ExplorePrompt {
        if let Some(index) = self.unread.iter().position(&wanted) {
            return self.unread.remove(index);
        }
        loop {
            let prompt = self.provider.next_prompt();
            if wanted(&prompt) {
                return prompt;
            }
            self.unread.push(prompt);
        }
    }

    fn agent_takes(&mut self, request: &review_explore::TurnRequest, update: &serde_json::Value) {
        let line = format!("Explore request: {}\n", request.request);
        let forks = self.fork_accesses.clone();
        let prompt = self.prompt_where(|prompt| {
            prompt.prompt.contains(&line) && !forks.contains(&prompt.access)
        });
        self.provider.reply(
            &prompt.access,
            "submit_question",
            &serde_json::json!({"update": update}),
        );
        loop {
            let event = self.run.flow.fixture.runtime.next_event();
            if let Some(committed) = event.downcast_ref::<ui_events::ExploreCommitted>() {
                let _ = committed.response.send(Ok(committed.applied));
                if committed
                    .round
                    .exploration
                    .conversation
                    .iter()
                    .any(|turn| turn.update.request == request.request)
                {
                    self.run.flow.exploration = committed.round.exploration.clone();
                    return;
                }
            }
        }
    }

    fn fork_prompts(&mut self, count: usize) -> Vec<ExplorePrompt> {
        let prompts: Vec<_> = (0..count)
            .map(|_| {
                let known = self.fork_accesses.clone();
                self.prompt_where(|prompt| !known.contains(&prompt.access))
            })
            .collect();
        self.fork_accesses
            .extend(prompts.iter().map(|prompt| prompt.access.clone()));
        prompts
    }

    fn forks_submit(&mut self, prompts: &[ExplorePrompt]) {
        for prompt in prompts {
            let question = fork_question(&self.run.flow, &prompt.prompt);
            self.provider.reply(
                &prompt.access,
                "submit_question",
                &serde_json::json!({"update": question}),
            );
        }
        let count = prompts.len();
        self.run.wait_for_saved("the forks' turns", |saved| {
            (saved
                .forks
                .iter()
                .filter(|fork| fork.turn.is_some())
                .count()
                == count)
                .then_some(())
        });
    }

    fn herdr_session(&self) -> Option<String> {
        self.herdr()
            .client()
            .get_agent(&self.herdr().pane_id)
            .unwrap()
            .and_then(|agent| agent.agent_session)
            .map(|session| session.value)
    }

    fn daemon(&self) -> Daemon {
        let processes = self
            .herdr()
            .client()
            .pane_process_info(&self.herdr().pane_id)
            .unwrap();
        Daemon::of_pane(&processes.foreground_processes).unwrap()
    }

    fn daemon_holds(&self, session: &str) -> bool {
        let rpc = self.daemon().connect(|_, _| {}).unwrap();
        rpc.request(
            "thread/read",
            &serde_json::json!({"threadId": session}),
            GUARD,
        )
        .is_ok()
    }

    fn agent_lists_threads(&self) -> String {
        review_test_support::codex::prompt(
            &self.herdr().server,
            &self.herdr().pane_id,
            "call the reviewer",
        );
        self.provider.next_output("call the reviewer")
    }

    fn approval_policy(&self, session: &str) -> serde_json::Value {
        let rpc = self.daemon().connect(|_, _| {}).unwrap();
        rpc.request(
            "thread/resume",
            &serde_json::json!({"threadId": session, "excludeTurns": true}),
            GUARD,
        )
        .unwrap()["approvalPolicy"]
            .clone()
    }

    fn first_question(&mut self) {
        let kickoff = self.run.flow.exploration.request(None, None).unwrap();
        let kickoff = self.flow().post_turn(&kickoff);
        self.agent_takes(&kickoff, &agent_update(&kickoff, 1));
    }

    fn answer(&mut self, choice: &str, text: &str) -> review_explore::TurnRequest {
        let question = self.run.flow.exploration.questions.last().cloned();
        let request = self
            .run
            .flow
            .exploration
            .request(
                Some(AnswerInput {
                    option: Some(choice.into()),
                    text: text.into(),
                    ..AnswerInput::default()
                }),
                question.as_ref(),
            )
            .unwrap();
        self.flow().post_turn(&request)
    }
}

fn install_plugin(codex: &CodexHome) {
    let status = std::process::Command::new(effects::fixture::test_fork_tools().control)
        .args(["codex-plugin", "install"])
        .envs(codex.environment())
        .status()
        .unwrap();
    assert!(status.success(), "codex-plugin install: {status}");
}

#[test]
fn a_bare_answer_continues_the_codex_agent_as_its_fork_and_the_next_answer_reaches_the_fork() {
    let mut codex = CodexRunAhead::start(Tui::OnDaemon);
    codex.first_question();
    let prompts = codex.fork_prompts(2);
    codex.forks_submit(&prompts);
    let saved = codex.run.saved();
    let agent_session = saved.forks[0].from.clone().unwrap();
    assert_eq!(
        codex.herdr_session().as_deref(),
        Some(agent_session.as_str()),
        "the forks leave Herdr's session for the pane where it was"
    );
    let session_for = |choice: &str| {
        saved
            .forks
            .iter()
            .find(|fork| fork.choice == choice)
            .unwrap()
            .session
            .clone()
    };
    let (fork, other) = (session_for("keep"), session_for("change"));

    let request = codex.answer("keep", "");

    let round = codex.run.wait_for_questions(2);
    let turn = &round.exploration.conversation.last().unwrap().update;
    assert_eq!(turn.request, request.request);
    codex.run.flow.exploration = round.exploration.clone();
    assert_eq!(
        codex.herdr_session().as_deref(),
        Some(fork.as_str()),
        "the reviewer told Herdr that the agent runs the fork's session"
    );
    codex.run.wait_for_saved("the other fork's end", |saved| {
        saved
            .fork(&other)
            .is_some_and(|fork| fork.cleaned)
            .then_some(())
    });
    assert!(!codex.daemon_holds(&other), "the other fork is deleted");
    assert!(
        codex.daemon_holds(&fork),
        "the fork's session is the agent's now"
    );
    assert_eq!(codex.approval_policy(&fork), "never");
    let listed = codex.agent_lists_threads();
    assert!(
        !listed.contains("blocked") && !listed.contains("rejected"),
        "the agent's own tool runs unasked on the fork's session: {listed}"
    );

    codex.fork_prompts(2);
    let next = codex.answer("keep", "Keep it, with a test.");
    codex.agent_takes(&next, &agent_update(&next, 3));
    let saved = codex.run.flow.saved();
    assert_eq!(saved.exploration.questions.len(), 3, "{}", codex.run.log());
}

#[test]
fn any_other_answer_runs_the_plain_chain_and_deletes_the_forks_from_the_daemon() {
    let mut codex = CodexRunAhead::start(Tui::OnDaemon);
    codex.first_question();
    let prompts = codex.fork_prompts(2);
    codex.forks_submit(&prompts);
    let forks: Vec<_> = codex
        .run
        .saved()
        .forks
        .iter()
        .map(|fork| fork.session.clone())
        .collect();

    let request = codex.answer("keep", "Keep it, with a test.");

    codex.agent_takes(&request, &agent_update(&request, 2));
    assert!(matches!(
        &codex.run.saved().answers.last().unwrap().path,
        review_run_ahead::TurnPath::Plain {
            reason: review_run_ahead::PlainReason::Comment
        }
    ));
    codex.run.wait_for_saved("the forks' end", |saved| {
        saved.forks.iter().all(|fork| fork.cleaned).then_some(())
    });
    for fork in &forks {
        assert!(!codex.daemon_holds(fork), "the fork {fork} is deleted");
    }
}

#[test]
fn text_left_in_codex_s_input_box_runs_no_turn_and_the_answer_waits_for_retry() {
    let mut codex = CodexRunAhead::start(Tui::OnDaemon);
    codex.first_question();
    let prompts = codex.fork_prompts(2);
    codex.forks_submit(&prompts);
    let agent_session = codex.run.saved().forks[0].from.clone().unwrap();
    let pane = codex.herdr().pane_id.clone();
    codex
        .herdr()
        .run_cli(&["pane", "send-text", &pane.0, "half a thought "]);
    review_test_support::codex::wait_output(&codex.herdr().server, &pane, "half a thought");

    let request = codex.answer("keep", "");

    let typed = codex.run.wait_for_saved("the failed switch", |saved| {
        saved.forks.iter().find_map(|fork| match &fork.continued {
            Some(Continuation::Failed { typed, .. }) => Some(*typed),
            _ => None,
        })
    });
    assert!(!typed, "the agent ran neither the text nor the /resume");
    for shown in [
        "half a thought /resume",
        "The progressive reviewer stopped this prompt",
    ] {
        review_test_support::codex::wait_output(&codex.herdr().server, &pane, shown);
    }
    assert!(matches!(
        &codex.run.saved().answers.last().unwrap().path,
        review_run_ahead::TurnPath::Plain {
            reason: review_run_ahead::PlainReason::SwitchFailed { .. }
        }
    ));
    assert_eq!(
        codex.herdr_session().as_deref(),
        Some(agent_session.as_str()),
        "the agent stays on its own session"
    );
    codex
        .herdr()
        .run_cli(&["pane", "send-keys", &pane.0, "ctrl+u"]);

    codex
        .run
        .flow
        .fixture
        .explore(ExploreCommand::Retry(Box::new(request.clone())));

    codex.agent_takes(&request, &agent_update(&request, 2));
    assert_eq!(codex.approval_policy(&agent_session), "never");
    let listed = codex.agent_lists_threads();
    assert!(
        !listed.contains("blocked") && !listed.contains("rejected"),
        "the agent's own tool runs unasked: {listed}"
    );
}

#[test]
fn a_codex_agent_without_the_daemon_runs_the_plain_chain_and_the_setting_says_why() {
    let mut codex = CodexRunAhead::start(Tui::WithoutDaemon);
    codex.first_question();

    let unavailable = codex
        .run
        .flow
        .fixture
        .runtime
        .wait_for::<ui_events::ExploreRunAheadUnavailable>();

    let why = unavailable.0.expect("run-ahead is not available");
    assert!(why.contains("--no-daemon"), "{why}");
    let request = codex.answer("keep", "");
    codex.agent_takes(&request, &agent_update(&request, 2));
    let saved = codex.run.saved();
    assert_eq!(saved.forks, Vec::new(), "{}", codex.run.log());
    assert!(matches!(
        &saved.answers.last().unwrap().path,
        review_run_ahead::TurnPath::Plain {
            reason: review_run_ahead::PlainReason::NoForks { why: Some(_) }
        }
    ));
}

#[test]
fn a_reopened_reviewer_interrupts_and_deletes_the_codex_forks_a_stopped_one_left() {
    let mut codex = CodexRunAhead::start(Tui::OnDaemon);
    codex.first_question();
    codex.fork_prompts(2);
    let left: Vec<_> = codex.run.wait_for_saved("the forks' turns", |saved| {
        saved
            .forks
            .iter()
            .all(|fork| codex_fork::turn(&fork.host).is_some())
            .then(|| saved.forks.clone())
    });
    let directory = codex.herdr().hook_directory();
    for fork in &left {
        assert!(codex.daemon_holds(&fork.session));
        assert!(
            directory.root().is_codex_fork(&fork.session),
            "the fork is guarded"
        );
    }
    let hooks = std::sync::Arc::new(agent_hooks::AgentHooks::listen(&directory).unwrap());
    let (herdr, log) = (
        std::sync::Arc::new(codex.herdr().client())
            as std::sync::Arc<dyn herdr_client::protocol::AgentHost>,
        review_run_ahead::RunAheadLog::default(),
    );
    let reopened = agent_forks::AgentForks::new(
        claude_fork::ClaudeForks::new(
            std::sync::Arc::clone(&herdr),
            effects::fixture::test_fork_tools(),
            log.clone(),
            test_waits(),
            Ok(std::sync::Arc::clone(&hooks)),
        ),
        codex_fork::CodexForks::new(herdr, log, test_waits(), Some(hooks)),
    );

    let (done, discarded) = std::sync::mpsc::channel();
    for fork in &left {
        let done = done.clone();
        review_run_ahead::ForkHost::discard(
            &reopened,
            fork.trace(),
            Box::new(move || {
                let _ = done.send(());
            }),
        );
    }

    for _ in &left {
        discarded.recv().unwrap();
    }
    for fork in &left {
        assert!(
            !codex.daemon_holds(&fork.session),
            "{} is deleted",
            fork.session
        );
        assert!(!directory.root().is_codex_fork(&fork.session));
    }
    codex
        .run
        .wait_for_saved("the forks' interrupted turns", |saved| {
            saved
                .forks
                .iter()
                .all(|fork| {
                    fork.exit
                        .as_deref()
                        .is_some_and(|exit| exit.contains("interrupted"))
                })
                .then_some(())
        });
}

#[test]
fn a_codex_agent_put_back_after_a_switch_it_never_confirmed_runs_its_own_tools_unasked() {
    let waits = review_run_ahead::ForkWaits {
        resume: Duration::ZERO,
        ..test_waits()
    };
    let mut codex = CodexRunAhead::start_with(Tui::OnDaemon, waits);
    codex.first_question();
    let prompts = codex.fork_prompts(2);
    codex.forks_submit(&prompts);
    let saved = codex.run.saved();
    let agent_session = saved.forks[0].from.clone().unwrap();
    let fork = saved
        .forks
        .iter()
        .find(|fork| fork.choice == "keep")
        .unwrap()
        .session
        .clone();
    let (heard, events) = std::sync::mpsc::channel();
    let _follow = codex
        .daemon()
        .follow(&fork, Duration::from_millis(20), move |event| {
            let _ = heard.send(event);
        });
    assert!(matches!(
        events.recv().unwrap(),
        codex_daemon::SessionEvent::Joined(_)
    ));

    let request = codex.answer("keep", "");

    let mut joins = 0;
    while joins < 2 {
        if events.recv().unwrap() == codex_daemon::SessionEvent::Resumed {
            joins += 1;
        }
    }
    codex.run.wait_for_saved("the agent put back", |saved| {
        saved
            .forks
            .iter()
            .any(|fork| matches!(fork.continued, Some(Continuation::Undone { .. })))
            .then_some(())
    });
    assert_eq!(
        codex.herdr_session().as_deref(),
        Some(agent_session.as_str())
    );
    codex
        .run
        .flow
        .fixture
        .explore(ExploreCommand::Retry(Box::new(request.clone())));
    codex.agent_takes(&request, &agent_update(&request, 2));
    assert_eq!(codex.approval_policy(&agent_session), "never");
    let listed = codex.agent_lists_threads();
    assert!(
        !listed.contains("blocked") && !listed.contains("rejected"),
        "the agent's own tool runs unasked: {listed}"
    );
}
