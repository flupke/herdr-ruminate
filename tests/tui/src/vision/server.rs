//! The vision MCP server: an agent's eyes and hands on the reviewer, running in herdr-common's
//! app sandbox. herdr-common's vision core gives the tools that see and act on the app (screen,
//! screenshot, key, type, click, drag, `wait_for`); this server adds the reviewer's own: its
//! session, its stand-in agent and the agent's forks, Jev's stand-in, and the Explore page.

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail, ensure};
use review_repository::repository::RepoType;
use rmcp::{
    ErrorData, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ContentBlock, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use vision_mcp::{Screen, Vision, VisionTools};

use super::scratch::Seed;
use super::session::{Session, StartOptions, from_eyre};

/// The longest a `turn` or `forks` may wait.
const MAX_WAIT_MS: u64 = 60_000;

/// What the server's own tools are; the vision core's instructions follow.
const INSTRUCTIONS: &str = "Eyes and hands on the reviewer's terminal pane. `start` builds the reviewer from this checkout and runs it in herdr-common's app sandbox, a private herdr-app on a headless display that never shows on the user's screen nor takes their focus, on a scratch repository, beside a stand-in implementation agent that run-ahead can fork. The reviewer's pane has the focus: key and type act on it; click and drag take the window's pixels, which screenshot shows. The reviewer speaks vision-signal, so every action returns once it has read the input and painted its reaction; the reviewer's own tools (jev, reply, fork_submit, fork_end, agent_prompt) return its screen once it painted what they did. For asynchronous work, call wait_for with the text the finished screen shows. With run-ahead on (key z in the Explore tab), forks lists the forks of the question that waits; fork_submit, fork_end (no turn, or a crash) or nothing (still working) scripts each fork, switch_unheard makes the next switch to a fork fail, and agent_working, agent_prompt and start's agent_hooks make the agent busy, move its conversation on, or run it without the reviewer's plugin hooks. Use note to record what you checked and found in the session's actions.jsonl. Call stop when done: it stops the sandbox and removes the scratch repository.";

/// The repository a session reviews.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum RepositoryKind {
    #[default]
    Jj,
    Git,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct StartInput {
    /// The scratch repository's version control, jj by default.
    #[serde(default)]
    repository: RepositoryKind,
    /// Files of the base change, by repository-relative path. With neither base nor change, a
    /// sample change (a renamed function, a Rust file, a Markdown file with Unicode and a long
    /// line) is reviewed.
    #[serde(default)]
    base: BTreeMap<String, String>,
    /// Files written on top of the base: the change under review.
    #[serde(default)]
    change: BTreeMap<String, String>,
    /// Run the stand-in agent with the hooks of the reviewer's Claude Code plugin, which
    /// run-ahead needs: true by default. Without them, run-ahead takes no fork, and each answer
    /// says nothing was prepared: under herdr-core the hook is what names the agent's session.
    agent_hooks: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct ReopenInput {
    /// Run the new stand-in agent with the hooks of the reviewer's Claude Code plugin, or
    /// without; as before when left out.
    agent_hooks: Option<bool>,
    /// Run the reviewer and its agent in this workspace of the scratch repository from now on:
    /// a jj workspace at the change under review (a Git worktree at HEAD) that the first reopen
    /// with the name adds beside the repository; `repository` goes back to the first one. In
    /// the same one as before when left out.
    workspace: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct JevInput {
    /// The repository-relative file whose hunks the stand-in Jev classifies as insignificant.
    path: String,
    /// One-based lines: each diff hunk of `path` that adds one of them (current numbering) or
    /// removes one (base numbering) is insignificant.
    lines: Vec<u32>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct TurnInput {
    /// Return the first turn after this number, instead of after the last turn returned.
    after: Option<u64>,
    /// The longest wait for the reviewer to send the turn, 15000 ms by default, at most 60000.
    timeout_ms: Option<u64>,
}

/// A tool of the reviewer's MCP endpoint that the stand-in agent calls.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum AgentTool {
    SubmitQuestion,
    SubmitConclusion,
    ReviseQuestion,
    Reply,
    GetThread,
    ListThreads,
    GetNewMessages,
}

impl AgentTool {
    fn name(self) -> &'static str {
        match self {
            Self::SubmitQuestion => "submit_question",
            Self::SubmitConclusion => "submit_conclusion",
            Self::ReviseQuestion => "revise_question",
            Self::Reply => "reply",
            Self::GetThread => "get_thread",
            Self::ListThreads => "list_threads",
            Self::GetNewMessages => "get_new_messages",
        }
    }
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct ReplyInput {
    /// The turn to answer, which must be the latest one.
    turn: u64,
    /// The reviewer's MCP tool to call as the stand-in agent.
    tool: AgentTool,
    /// The tool's arguments, as the reviewer's MCP tool takes them (examples in
    /// tests/tui/examples). The turn's access value (`review`), and its `instance`, `request` and
    /// `checkpoint` (inside `update` for `submit_question`) are filled in unless given;
    /// `revise_question`, answering a comments turn, takes the access value, round and
    /// checkpoint of the latest Explore turn, inside `revision`; an
    /// `interpretation` without an `answer` is about the answer the turn brought. Arguments with
    /// their own `review` are sent as they are, to check how the reviewer rejects them.
    arguments: serde_json::Map<String, Value>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct ForksInput {
    /// Wait until at least this many forks started, counting those of earlier questions, instead
    /// of returning at once.
    count: Option<usize>,
    /// The longest the wait goes on with no new event of the stand-in agent or its forks, 15000
    /// ms by default, at most 60000.
    timeout_ms: Option<u64>,
}

/// A tool of the reviewer's MCP endpoint that a fork submits its turn with.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum ForkTool {
    SubmitQuestion,
    SubmitConclusion,
}

impl ForkTool {
    fn name(self) -> &'static str {
        match self {
            Self::SubmitQuestion => "submit_question",
            Self::SubmitConclusion => "submit_conclusion",
        }
    }
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct ForkEndInput {
    /// The fork's session, as forks lists it.
    session: String,
    /// Crash instead: the fork exits with a failure and prints no result, as a Claude Code that
    /// crashed. False by default: the fork ends its turn without a submit.
    #[serde(default)]
    crash: bool,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct SwitchUnheardInput {
    /// The reviewer's hooks hear of the resume this many milliseconds late, instead of never: a
    /// switch heard within 20000 ms succeeds. At most 60000.
    heard_after_ms: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct AgentWorkingInput {
    /// Show the agent working (true) or idle again (false).
    working: bool,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct AgentPromptInput {
    /// What the user submits in the agent's terminal; `/resume <session>` resumes that
    /// session.
    text: String,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct ForkSubmitInput {
    /// The fork's session, as forks lists it.
    session: String,
    /// The reviewer's MCP tool the fork submits its turn with.
    tool: ForkTool,
    /// The tool's arguments, as for reply: the access value, `instance`, `request`, `checkpoint`
    /// and the interpretation's `answer` are filled in from the fork's prompt unless given.
    arguments: serde_json::Map<String, Value>,
}

/// What a note records.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum NoteKind {
    Checked,
    Finding,
    Untested,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct NoteInput {
    kind: NoteKind,
    /// For a finding: the expected behavior, the observed behavior, and the steps to reproduce.
    text: String,
}

#[derive(Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct NoInput {}

/// Where a session's reviewer comes from, and where its files go.
#[derive(Clone)]
pub struct Setup {
    /// The checkout whose reviewer `start` builds.
    pub checkout: PathBuf,
    /// The directory under which each session gets a new directory.
    pub sessions: PathBuf,
}

impl Setup {
    /// This checkout, and `tests/tui/target/vision`.
    pub fn from_env() -> Self {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        Self {
            checkout: manifest.join("../.."),
            sessions: manifest.join("target/vision"),
        }
    }
}

#[derive(Clone)]
pub(crate) struct Server {
    setup: Setup,
    /// The reviewer's side of the running session.
    session: Arc<Mutex<Option<Session>>>,
    /// The vision core's tools, on the session's app sandbox.
    vision: VisionTools,
    router: ToolRouter<Self>,
}

/// What a tool returns: a summary, and the reviewer's screen.
struct Outcome {
    summary: Value,
    screen: Option<Screen>,
}

impl Outcome {
    fn summary(summary: Value) -> Self {
        Self {
            summary,
            screen: None,
        }
    }

    fn screen(status: &str, screen: Screen) -> Self {
        Self {
            summary: json!({"status": status}),
            screen: Some(screen),
        }
    }

    fn with(mut self, key: &str, value: impl Serialize) -> Self {
        self.summary[key] = json!(value);
        self
    }

    fn into_result(mut self) -> CallToolResult {
        if let Some(screen) = &self.screen {
            self.summary["terminal"] = json!(screen.terminal);
            if let Some(frame) = screen.frame {
                self.summary["frame"] = json!(frame);
            }
        }
        let mut content = vec![ContentBlock::text(self.summary.to_string())];
        if let Some(screen) = self.screen {
            content.push(ContentBlock::text(screen.numbered()));
        }
        CallToolResult::success(content)
    }
}

/// The running session: the reviewer's side, and the vision core on its sandbox.
struct Running<'a> {
    session: &'a mut Session,
    vision: &'a mut Vision,
}

impl Server {
    pub(crate) fn new(setup: Setup) -> Self {
        Self {
            setup,
            session: Arc::default(),
            vision: VisionTools::new(),
            router: Self::reviewer_router() + VisionTools::router(|server: &Self| &server.vision),
        }
    }

    /// Run `action` on the session off the async runtime, record it in the session's
    /// transcript, and turn an error into an error result the agent reads.
    async fn run(
        &self,
        tool: &'static str,
        arguments: Value,
        action: impl FnOnce(&Setup, &mut Option<Session>, &mut Option<Vision>) -> Result<Outcome>
        + Send
        + 'static,
    ) -> Result<CallToolResult, ErrorData> {
        let setup = self.setup.clone();
        let session = Arc::clone(&self.session);
        let vision = Arc::clone(self.vision.session());
        tokio::task::spawn_blocking(move || {
            // The reviewer's side first, then the core's, which the core's tools lock alone.
            let mut session = session.lock().unwrap_or_else(PoisonError::into_inner);
            let mut vision = vision.lock().unwrap_or_else(PoisonError::into_inner);
            let outcome = action(&setup, &mut session, &mut vision);
            if let Some(session) = session.as_ref() {
                record(session, tool, &arguments, &outcome);
            }
            match outcome {
                Ok(outcome) => outcome.into_result(),
                Err(error) => CallToolResult::error(vec![ContentBlock::text(format!("{error:#}"))]),
            }
        })
        .await
        .map_err(|error| ErrorData::internal_error(error.to_string(), None))
    }

    /// Run `action` on the running session.
    async fn on_session(
        &self,
        tool: &'static str,
        arguments: Value,
        action: impl FnOnce(Running<'_>) -> Result<Outcome> + Send + 'static,
    ) -> Result<CallToolResult, ErrorData> {
        self.run(tool, arguments, move |_, session, vision| {
            let session = session
                .as_mut()
                .context("no session is running; call start first")?;
            let vision = vision
                .as_mut()
                .context("the session's sandbox is gone; call stop, then start")?;
            action(Running { session, vision })
        })
        .await
    }
}

#[tool_router(router = reviewer_router)]
impl Server {
    #[tool(
        description = "Start a vision session: build the reviewer from this checkout, create a scratch repository with a base change and a change under review, and run the reviewer in herdr-common's app sandbox (a private herdr-app on a headless display) beside a stand-in implementation agent (a program named claude that Explore and review comments prompt, which reports its session and runs the reviewer's Claude Code plugin hooks unless agent_hooks is false, so run-ahead can fork it). Returns the reviewer's first screen, which may come before the repository has loaded (wait_for a file name), its terminal and the agent's, the repository (edit its files to exercise filesystem-driven updates) and the session directory. One session at a time."
    )]
    async fn start(
        &self,
        Parameters(input): Parameters<StartInput>,
    ) -> Result<CallToolResult, ErrorData> {
        let arguments = json!(input);
        self.run("start", arguments, move |setup, session, vision| {
            ensure!(session.is_none(), "a session is running; stop it first");
            let reviewer = build_reviewer(&setup.checkout)?;
            let seed = if input.base.is_empty() && input.change.is_empty() {
                Seed::sample()
            } else {
                Seed {
                    base: input.base,
                    change: input.change,
                }
            };
            let options = StartOptions {
                repository: match input.repository {
                    RepositoryKind::Jj => RepoType::Jj,
                    RepositoryKind::Git => RepoType::Git,
                },
                seed,
                agent_hooks: input.agent_hooks.unwrap_or(true),
            };
            let directory = setup.sessions.join(format!(
                "{}-{}",
                SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos(),
                std::process::id()
            ));
            let (started, core, screen) = Session::start(reviewer, &options, directory)?;
            let outcome =
                Outcome::screen("started", screen).with("session", started.describe(&core));
            *session = Some(started);
            *vision = Some(core);
            Ok(outcome)
        })
        .await
    }

    #[tool(
        description = "Stand in for the paid Jev classifier, which vision sessions never reach: classify as insignificant every diff hunk of path that adds one of lines (current numbering) or removes one (base numbering), press rf, and return the reviewer's screen once Jev has finished, with its result (\"Jev: marked\"), or once rf refused. Nearby edits share one diff hunk, as they do for Jev. The script is kept as jev-script.json in the session directory: write it yourself before an Explore round to have Jev mark at the round's start."
    )]
    async fn jev(
        &self,
        Parameters(input): Parameters<JevInput>,
    ) -> Result<CallToolResult, ErrorData> {
        self.on_session("jev", json!(input), move |running| {
            Ok(Outcome::screen(
                "classified",
                running
                    .session
                    .jev(running.vision, &input.path, &input.lines)?,
            ))
        })
        .await
    }

    #[tool(
        description = "Wait for the next prompt the reviewer sent to its agent, and return it as a turn: its number, kind (kickoff, wakeup, implement for Explore; comments for a review thread's wakeup), text, and the access value and identity a reply needs. Each call returns the turn after the last one returned, or after `after`. The reviewer records turns only in a vision session."
    )]
    async fn turn(
        &self,
        Parameters(input): Parameters<TurnInput>,
    ) -> Result<CallToolResult, ErrorData> {
        self.on_session("turn", json!(input), move |running| {
            let guard = wait_guard(input.timeout_ms, 15_000)?;
            let turn = running.session.next_turn(input.after, guard)?;
            Ok(Outcome::summary(json!({"status": "turn", "turn": turn})))
        })
        .await
    }

    #[tool(
        description = "Answer the latest turn as the stand-in agent: call one of the reviewer's MCP tools on its real endpoint (submit_question or submit_conclusion for Explore; get_new_messages, then reply, for a comments turn). Returns the tool's result as `reply`, and the reviewer's screen once it has shown it. A turn that is not the latest is an error, so a script never answers a prompt the reviewer replaced; the reviewer's own validation errors come back in `reply`."
    )]
    async fn reply(
        &self,
        Parameters(input): Parameters<ReplyInput>,
    ) -> Result<CallToolResult, ErrorData> {
        self.on_session("reply", json!(input), move |running| {
            let (reply, screen) = running.session.reply(
                running.vision,
                input.turn,
                input.tool.name(),
                Value::Object(input.arguments),
            )?;
            Ok(Outcome::screen("replied", screen).with("reply", reply))
        })
        .await
    }

    #[tool(
        description = "List the forks run-ahead started of the stand-in agent's session, oldest first: each fork's session, the session it copies, its choice, its prompt with the turn's files (its access and Answer ID among them), and whether it submitted, ended its turn, or exited (the reviewer stops the forks of a question it no longer needs). Also returns the agent's sessions (the one it started last, once its SessionStart hooks ran, and those it resumed: after a bare answer to a prepared choice, the fork's), and run-ahead's log. With count, waits until that many forks started."
    )]
    async fn forks(
        &self,
        Parameters(input): Parameters<ForksInput>,
    ) -> Result<CallToolResult, ErrorData> {
        self.on_session("forks", json!(input), move |running| {
            let guard = wait_guard(input.timeout_ms, 15_000)?;
            let forks = running.session.forks(input.count, guard)?;
            Ok(Outcome::summary(
                json!({"status": "forks", "run_ahead": forks}),
            ))
        })
        .await
    }

    #[tool(
        description = "Submit a fork's turn as that fork: call submit_question or submit_conclusion on the reviewer's real endpoint with the fork's own access, then make the fork's output show the call and its result, as Claude Code's would. The reviewer keeps that turn, shown to nobody, until a bare answer with the fork's choice uses it. Returns the tool's result as `reply`, and the reviewer's screen once it has shown it; the reviewer's own validation errors come back in `reply`, and the fork is then not told."
    )]
    async fn fork_submit(
        &self,
        Parameters(input): Parameters<ForkSubmitInput>,
    ) -> Result<CallToolResult, ErrorData> {
        self.on_session("fork_submit", json!(input), move |running| {
            let (reply, screen) = running.session.fork_submit(
                running.vision,
                &input.session,
                input.tool.name(),
                Value::Object(input.arguments),
            )?;
            let status = if reply["isError"] == true {
                "refused"
            } else {
                "submitted"
            };
            Ok(Outcome::screen(status, screen).with("reply", reply))
        })
        .await
    }

    #[tool(
        description = "End a fork's turn without a submit (no turn), or make it crash with crash (it exits with a failure and no result). Either way the fork's process ends, and the reviewer counts a failure of run-ahead: three in a row stop it for the round. A bare answer that waits for this fork, or comes after, goes to the agent in the pane. Returns the reviewer's screen once the fork's process ended; wait_for what the reviewer shows of it."
    )]
    async fn fork_end(
        &self,
        Parameters(input): Parameters<ForkEndInput>,
    ) -> Result<CallToolResult, ErrorData> {
        self.on_session("fork_end", json!(input), move |running| {
            let screen = running
                .session
                .fork_end(running.vision, &input.session, input.crash)?;
            Ok(Outcome::screen("ended", screen))
        })
        .await
    }

    #[tool(
        description = "Make the stand-in agent's next /resume one that the reviewer's hooks never hear of, as when Claude Code's session hooks fail, or hear of only heard_after_ms later, as when they come late; the agent reads nothing typed meanwhile. The next switch to a fork (a bare answer to a submitted choice) waits for them: unheard, it fails after 20 s, the turn shows as interrupted with Retry, and the agent is told to resume its own session again, which is heard. Stop waiting and Cancel answer can be played while the switch waits."
    )]
    async fn switch_unheard(
        &self,
        Parameters(input): Parameters<SwitchUnheardInput>,
    ) -> Result<CallToolResult, ErrorData> {
        self.on_session("switch_unheard", json!(input), move |running| {
            if let Some(late) = input.heard_after_ms {
                ensure!(
                    late <= MAX_WAIT_MS,
                    "heard_after_ms is at most {MAX_WAIT_MS}"
                );
            }
            let heard_after = input.heard_after_ms.map(Duration::from_millis);
            running.session.switch_unheard(heard_after)?;
            Ok(Outcome::summary(json!({"status": "next switch unheard"})))
        })
        .await
    }

    #[tool(
        description = "Show the stand-in agent working without a turn, as an agent at work on what no prompt started, or idle again, and return the reviewer's screen once the app shows it so. While it works, run-ahead takes no fork of a new question, and an answer finds it working; a prompt the reviewer sends it waits until it is idle."
    )]
    async fn agent_working(
        &self,
        Parameters(input): Parameters<AgentWorkingInput>,
    ) -> Result<CallToolResult, ErrorData> {
        self.on_session("agent_working", json!(input), move |running| {
            let screen = running
                .session
                .agent_working(running.vision, input.working)?;
            Ok(Outcome::screen("shown", screen))
        })
        .await
    }

    #[tool(
        description = "Submit a prompt in the stand-in agent's terminal, as its user would, without moving the focus, and return once the agent read it, with the reviewer's screen. The agent takes a turn on it, which moves its conversation on past the forks of the question that waits."
    )]
    async fn agent_prompt(
        &self,
        Parameters(input): Parameters<AgentPromptInput>,
    ) -> Result<CallToolResult, ErrorData> {
        self.on_session("agent_prompt", json!(input), move |running| {
            let screen = running.session.agent_prompt(running.vision, &input.text)?;
            Ok(Outcome::screen("read", screen))
        })
        .await
    }

    #[tool(
        description = "Run the Herdr action that opens the Explore page, as Herdr would run it in the reviewer's space, with a browser that records the address instead of opening it. Returns that address, with its token, as page: open it in a browser on this machine, since the page draws itself from its socket (curl gets no round)."
    )]
    async fn explore_page(
        &self,
        Parameters(_): Parameters<NoInput>,
    ) -> Result<CallToolResult, ErrorData> {
        self.on_session("explore_page", json!({}), move |running| {
            let page = running.session.explore_page()?;
            Ok(Outcome::summary(json!({"status": "opened", "page": page})))
        })
        .await
    }

    #[tool(
        description = "Record what you checked, a finding (expected behavior, observed behavior, steps to reproduce), or an area left untested, in the session's actions.jsonl."
    )]
    async fn note(
        &self,
        Parameters(input): Parameters<NoteInput>,
    ) -> Result<CallToolResult, ErrorData> {
        self.on_session("note", json!(input), move |running| {
            let directory = running.session.directory().to_owned();
            Ok(Outcome::summary(
                json!({"status": "recorded", "transcript": directory.join("actions.jsonl")}),
            ))
        })
        .await
    }

    #[tool(
        description = "Start the reviewer and its stand-in agent again, in a new tab of the sandbox's space, on the same repository and state, and return the reviewer's first screen and the new terminals. With agent_hooks, the new agent runs with or without the hooks of the reviewer's Claude Code plugin. With workspace, both run in that workspace of the scratch repository, which the first such reopen adds, at the change under review: the review state is the one the repository's workspaces share."
    )]
    async fn reopen(
        &self,
        Parameters(input): Parameters<ReopenInput>,
    ) -> Result<CallToolResult, ErrorData> {
        self.on_session("reopen", json!(input), move |running| {
            let screen = running.session.reopen(
                running.vision,
                input.agent_hooks,
                input.workspace.as_deref(),
            )?;
            Ok(Outcome::screen("reopened", screen)
                .with("session", running.session.describe(running.vision)))
        })
        .await
    }

    #[tool(
        description = "Stop the session: stop its app sandbox, with the reviewer and the stand-in agent, and remove the scratch repository. The session directory, with actions.jsonl, stays."
    )]
    async fn stop(&self, Parameters(_): Parameters<NoInput>) -> Result<CallToolResult, ErrorData> {
        self.run("stop", json!({}), move |_, session, vision| {
            let stopped = session.take().context("no session is running")?;
            let left = match vision.take() {
                Some(vision) => vision.stop().map_err(from_eyre)?.processes,
                None => Vec::new(),
            };
            let directory = stopped.directory().to_owned();
            let transcript = directory.join("actions.jsonl");
            drop(stopped);
            let outcome = Outcome::summary(json!({
                "status": "stopped",
                "processes_left": left,
                "directory": directory,
            }));
            append(&transcript, "stop", &json!({}), &Ok(&outcome));
            Ok(outcome)
        })
        .await
    }
}

#[tool_handler(router = self.router)]
impl ServerHandler for Server {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            format!(
                "{INSTRUCTIONS}\n\nThe vision core: {}",
                vision_mcp::INSTRUCTIONS
            ),
        )
    }
}

fn wait_guard(timeout_ms: Option<u64>, default: u64) -> Result<Duration> {
    let timeout = timeout_ms.unwrap_or(default);
    ensure!(
        timeout <= MAX_WAIT_MS,
        "timeout_ms is at most {MAX_WAIT_MS}"
    );
    Ok(Duration::from_millis(timeout))
}

/// Append one tool call to the session's `actions.jsonl`.
fn record(session: &Session, tool: &str, arguments: &Value, outcome: &Result<Outcome>) {
    append(
        &session.directory().join("actions.jsonl"),
        tool,
        arguments,
        &outcome.as_ref().map_err(|error| format!("{error:#}")),
    );
}

fn append(
    transcript: &std::path::Path,
    tool: &str,
    arguments: &Value,
    outcome: &std::result::Result<&Outcome, String>,
) {
    let entry = match outcome {
        Ok(outcome) => json!({
            "tool": tool, "arguments": arguments, "result": outcome.summary,
            "frame": outcome.screen.as_ref().and_then(|screen| screen.frame),
            "screen": outcome.screen.as_ref().map(|screen| &screen.text),
        }),
        Err(error) => json!({"tool": tool, "arguments": arguments, "error": error}),
    };
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(transcript)
    {
        let _ = writeln!(file, "{entry}");
    }
}

/// Build the reviewer of `checkout`, as the session runs it, and return its path; and the
/// stand-in agent in this workspace, beside the server, which the session installs. The builds
/// run in the checkout's dev shell, as `make` does, since the server runs outside it.
fn build_reviewer(checkout: &std::path::Path) -> Result<PathBuf> {
    let build = |arguments: &[&str]| -> Result<()> {
        let output = std::process::Command::new(checkout.join("scripts/dev-shell"))
            .args(["cargo", "build", "--locked", "--message-format", "short"])
            .args(arguments)
            .current_dir(checkout)
            .output()
            .context("cannot run cargo in the dev shell")?;
        if !output.status.success() {
            bail!(
                "the reviewer does not build:\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(())
    };
    build(&[
        "-p",
        "reviewer",
        "--bin",
        "reviewer",
        "--bin",
        "reviewer-control",
    ])?;
    build(&[
        "--manifest-path",
        "tests/tui/Cargo.toml",
        "-p",
        "stand-in-agent",
        "--bin",
        "stand-in-agent",
    ])?;
    let target =
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| checkout.join("target"), PathBuf::from);
    Ok(target.join("debug/reviewer"))
}

/// Serve the vision tools on standard input and output until the client leaves or a signal
/// stops the server; a running session stops then.
pub fn serve(setup: Setup) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let server = Server::new(setup);
    let session = Arc::clone(&server.session);
    let vision = Arc::clone(server.vision.session());
    let outcome = runtime.block_on(async move {
        use tokio::signal::unix::{SignalKind, signal};
        let running = server.serve(rmcp::transport::stdio()).await?;
        let mut terminate = signal(SignalKind::terminate())?;
        let mut hangup = signal(SignalKind::hangup())?;
        let mut interrupt = signal(SignalKind::interrupt())?;
        tokio::select! {
            waited = running.waiting() => {
                waited?;
            }
            _ = terminate.recv() => {}
            _ = hangup.recv() => {}
            _ = interrupt.recv() => {}
        }
        anyhow::Ok(())
    });
    session
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take();
    if let Some(vision) = vision.lock().unwrap_or_else(PoisonError::into_inner).take() {
        let _ = vision.stop();
    }
    outcome
}
