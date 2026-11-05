//! One vision session: the reviewer in herdr-common's app sandbox, a private herdr-app on a
//! headless display, on a scratch repository, beside a stand-in agent that run-ahead can fork.
//! The vision core (`vision_mcp::Vision`) sees and drives the app; this session holds what is the
//! reviewer's own: its scratch repository, its terminals, and the stand-in agent and its forks,
//! which report on the session's event socket.

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, ensure};
use app_sandbox::{AppSandbox, Isolation, Options, Programs};
use fake_agent::{Command, Event, Events, Reported, Role};
use review_repository::repository::RepoType;
use serde_json::{Value, json};
use vision_mcp::{Input, Screen, Vision};

use super::agent::ScriptedAgent;
use super::forks;
use super::scratch::{Scratch, Seed};

/// How long the reviewer may take to read an input and paint its reaction, or to paint its first
/// frame, and the stand-in agent to answer. It fires only when something went wrong.
const REACTION_GUARD: Duration = Duration::from_secs(30);

/// The app's window: wide enough for the reviewer beside its agent.
const DISPLAY_SIZE: (u16, u16) = (1600, 1000);

/// The stand-in agent's name on the event socket.
pub(crate) const AGENT: &str = "agent";

/// The event socket the stand-in agent and its forks report on, in the sandbox's directory: its
/// path there is short, as a socket's must be, and both sides reach it.
const EVENTS_SOCKET: &str = "stand-in.sock";

/// The prefix of the reviewer's MCP tools, as Claude Code names them in a fork's output.
const MCP_TOOL_PREFIX: &str = "mcp__herdr_reviewer__";

/// What a session starts on.
pub(crate) struct StartOptions {
    pub(crate) repository: RepoType,
    pub(crate) seed: Seed,
    /// Whether the stand-in agent runs the hooks of the reviewer's Claude Code plugin, which
    /// run-ahead needs.
    pub(crate) agent_hooks: bool,
}

/// The session's terminals in the app, by their hc addresses (`t2`), and their panes.
struct Terminals {
    agent: String,
    agent_pane: String,
    reviewer: String,
    reviewer_pane: String,
}

pub(crate) struct Session {
    /// The "reviewer vision" space, which the reviewer's Explore page is the page of.
    space: String,
    terminals: Terminals,
    /// What the stand-in agent and its forks report.
    events: Events,
    scratch: Scratch,
    reviewer_program: PathBuf,
    /// Whether the stand-in agent runs the hooks of the reviewer's Claude Code plugin.
    agent_hooks: bool,
    agent: ScriptedAgent,
}

impl Session {
    /// Start an app sandbox on a new scratch repository, with the reviewer `reviewer` beside its
    /// stand-in agent in a "reviewer vision" space, and wait for the reviewer's first frame.
    /// Returns the session, the vision core on the sandbox, and the reviewer's first screen.
    pub(crate) fn start(
        reviewer: PathBuf,
        options: &StartOptions,
        directory: PathBuf,
    ) -> Result<(Self, Vision, Screen)> {
        let scratch = Scratch::create(options.repository, &options.seed, directory)?;
        scratch.install_agent()?;
        let sandbox = AppSandbox::start(
            &Programs::pinned().map_err(from_eyre)?,
            &Options {
                isolation: Isolation::Bubblewrap,
                display_size: DISPLAY_SIZE,
                environment: scratch.sandbox_environment(),
            },
        )
        .map_err(from_eyre)?;
        // The server calls the reviewer's MCP endpoint as the agent and its forks, and the
        // Explore page's network listener is for a browser on the host.
        for port in [scratch.mcp_port(), scratch.page_port()] {
            sandbox.forward(port).map_err(from_eyre)?;
        }
        let events = Events::listen(&sandbox.directory().join(EVENTS_SOCKET))?;
        let vision = Vision::start(sandbox).map_err(from_eyre)?;
        let space = vision
            .sandbox()
            .herdr_app(&[
                "do",
                "space.new",
                &json!({"name": "reviewer vision", "folder": scratch.root()}).to_string(),
            ])
            .map_err(from_eyre)?;
        let terminals = open_terminals(&vision, &scratch, &reviewer, options.agent_hooks, &events)?;
        // The space's own first tab, a shell, once the session's tab took the focus.
        do_command(&vision, "tab.close", &json!({"id": string(&space["tab"])?}))?;
        let agent = ScriptedAgent::new(scratch.turns(), scratch.mcp_port());
        let session = Self {
            space: string(&space["space"])?,
            terminals,
            events,
            scratch,
            reviewer_program: reviewer,
            agent_hooks: options.agent_hooks,
            agent,
        };
        let screen = session.first_screen(&vision)?;
        Ok((session, vision, screen))
    }

    /// The reviewer's first screen, once the app also knows the stand-in agent: Explore prompts
    /// it from the start.
    fn first_screen(&self, vision: &Vision) -> Result<Screen> {
        wait_app(
            vision,
            "the app to see the stand-in agent's status",
            |listed| {
                terminal_entry(listed, &self.terminals.agent)
                    .is_some_and(|t| !t["status"].is_null())
            },
        )?;
        // Each frame shows the reviewer's tabs.
        vision
            .wait_for(
                "Explore",
                false,
                REACTION_GUARD,
                Some(&self.terminals.reviewer),
            )
            .map_err(|error| anyhow!("the reviewer did not start: {error:#}"))
    }

    /// What `start` and `reopen` report besides the screen.
    pub(crate) fn describe(&self, vision: &Vision) -> Value {
        json!({
            "space": self.space,
            "reviewer_terminal": self.terminals.reviewer,
            "agent_terminal": self.terminals.agent,
            "repository": self.scratch.root(),
            "directory": self.scratch.directory(),
            "sandbox": vision.sandbox().directory(),
            "agent_swallows_prompts": self.scratch.agent_directory().swallow(),
            "agent_hooks": self.agent_hooks,
            "run_ahead_log": self.scratch.run_ahead_log(),
            "claude_config": self.scratch.claude_config(),
            "page_port": self.scratch.page_port(),
        })
    }

    pub(crate) fn directory(&self) -> &Path {
        self.scratch.directory()
    }

    /// Ask the reviewer for an acknowledgement, with no input, and return the screen of the
    /// frame that names it: the reviewer's reaction to what reached it meanwhile, such as an
    /// agent's tool call. The reviewer takes the focus first: the request goes to the focused
    /// terminal.
    pub(crate) fn settle(&self, vision: &mut Vision) -> Result<Screen> {
        self.focus_reviewer(vision)?;
        let (_, screen) = vision.settle().map_err(from_eyre)?;
        screen.context("the reviewer's pane shows no terminal")
    }

    fn focus_reviewer(&self, vision: &Vision) -> Result<()> {
        if vision.focused_terminal().map_err(from_eyre)?.as_deref()
            != Some(self.terminals.reviewer.as_str())
        {
            do_command(
                vision,
                "pane.focus",
                &json!({"pane": self.terminals.reviewer_pane}),
            )?;
        }
        Ok(())
    }

    /// Classify as insignificant the hunks of `path` that hold `lines`, as Jev would, press
    /// `rf`, and return the screen once Jev's classification has finished, or `rf` refused.
    pub(crate) fn jev(&self, vision: &mut Vision, path: &str, lines: &[u32]) -> Result<Screen> {
        fs::write(
            self.scratch.jev_script(),
            serde_json::to_vec(&json!({"insignificant": [{"path": path, "lines": lines}]}))?,
        )?;
        self.focus_reviewer(vision)?;
        let (_, pressed) = vision
            .act(&Input::Keys(&["r".into(), "f".into()]))
            .map_err(from_eyre)?;
        let pressed = pressed.context("the reviewer's pane shows no terminal")?;
        if pressed.text.contains("Jev: classifying") {
            return vision
                .wait_for(
                    "Jev: classifying",
                    true,
                    REACTION_GUARD,
                    Some(&self.terminals.reviewer),
                )
                .map_err(from_eyre);
        }
        Ok(pressed)
    }

    pub(crate) fn next_turn(&mut self, after: Option<u64>, guard: Duration) -> Result<Value> {
        self.agent.next_turn(after, guard)
    }

    /// Answer turn `turn` with `tool`, and return the tool's result with the screen once the
    /// reviewer has shown it.
    pub(crate) fn reply(
        &mut self,
        vision: &mut Vision,
        turn: u64,
        tool: &str,
        arguments: Value,
    ) -> Result<(Value, Screen)> {
        let result = self.agent.reply(turn, tool, arguments)?;
        Ok((result, self.settle(vision)?))
    }

    /// The forks run-ahead started, once `count` did when it is given, waiting up to `guard`;
    /// the agent's sessions; and run-ahead's log.
    pub(crate) fn forks(&self, count: Option<usize>, guard: Duration) -> Result<Value> {
        if let Some(count) = count {
            self.events
                .try_wait_until(&format!("{count} forks"), guard, |received| {
                    forks::forks(received).len() >= count
                })
                .map_err(|error| anyhow!(error))?;
        }
        // A wait whose condition held at once took in none of the events that came since.
        let received = self.received()?;
        Ok(json!({
            "forks": forks::forks(&received),
            "agent": forks::agent_sessions(&received),
            "log": fs::read_to_string(self.scratch.run_ahead_log()).unwrap_or_default(),
        }))
    }

    /// Submit the turn of the fork `session` with `tool`, as that fork: the reviewer's tool on
    /// its real endpoint, with the identity of the fork's prompt where `arguments` leave it out;
    /// then the fork's output shows the call and its result. Returns the tool's result once the
    /// fork showed them, with the screen once the reviewer has shown it.
    pub(crate) fn fork_submit(
        &mut self,
        vision: &mut Vision,
        session: &str,
        tool: &str,
        arguments: Value,
    ) -> Result<(Value, Screen)> {
        let prompt = self.fork(session)?.prompt;
        let result = self.agent.reply_as_fork(&prompt, tool, arguments)?;
        if result["isError"] == true {
            return Ok((result, self.settle(vision)?));
        }
        self.obey(
            &Role::fork(session),
            &Command::CallTool {
                name: format!("{MCP_TOOL_PREFIX}{tool}"),
            },
        )?;
        Ok((result, self.settle(vision)?))
    }

    /// End the turn of the fork `session` without a submit, or make it crash when `crash`: it
    /// exits with a failure and no result. Returns the screen once the fork's process ended.
    pub(crate) fn fork_end(
        &mut self,
        vision: &mut Vision,
        session: &str,
        crash: bool,
    ) -> Result<Screen> {
        ensure!(
            !self.fork(session)?.exited,
            "the fork {session} has exited already"
        );
        let fork = Role::fork(session);
        let command = if crash {
            Command::Crash
        } else {
            Command::EndTurn
        };
        self.events
            .try_send(&fork, &command, REACTION_GUARD)
            .map_err(|error| anyhow!(error))?;
        self.events
            .try_wait_until("the fork's exit", REACTION_GUARD, |received| {
                received
                    .iter()
                    .any(|reported| reported.from == fork && reported.event == Event::Disconnected)
            })
            .map_err(|error| anyhow!(error))?;
        self.settle(vision)
    }

    fn fork(&self, session: &str) -> Result<forks::Fork> {
        forks::forks(&self.received()?)
            .into_iter()
            .find(|fork| fork.session == session)
            .with_context(|| format!("no fork has the session {session}"))
    }

    /// Make the agent's next `/resume` one that the reviewer's hooks never hear of, as when
    /// Claude Code's session hooks fail, or hear of only `heard_after` later: the next switch to
    /// a fork waits for them, in vain or that long.
    pub(crate) fn switch_unheard(&self, heard_after: Option<Duration>) -> Result<()> {
        let agent = self.scratch.agent_directory();
        fs::create_dir_all(agent.path())?;
        let late = heard_after.map_or_else(String::new, |late| late.as_millis().to_string());
        fs::write(agent.unreported_resume(), late)?;
        Ok(())
    }

    /// Submit `text` in the stand-in agent's terminal, as its user would, and return once the
    /// agent read it: a prompt moves its conversation on.
    pub(crate) fn agent_prompt(&self, vision: &mut Vision, text: &str) -> Result<Screen> {
        ensure!(
            !text.trim().is_empty(),
            "the agent's prompt is empty: the agent would not read it"
        );
        let from = self.received()?.len();
        let agent = Role::agent(AGENT);
        // herdr-core pastes it and presses Enter, without moving the focus.
        vision
            .sandbox()
            .hc(&["send", "--force", &self.terminals.agent, text])
            .map_err(from_eyre)?;
        self.events
            .try_wait_until("the agent's prompt", REACTION_GUARD, |received| {
                received[from..].iter().any(|reported| {
                    reported.from == agent
                        && matches!(
                            reported.event,
                            Event::PromptReceived { .. }
                                | Event::PromptBlocked { .. }
                                | Event::ResumeReceived { .. }
                        )
                })
            })
            .map_err(|error| anyhow!(error))?;
        self.settle(vision)
    }

    /// Run the Herdr action that opens the Explore page in the sandbox, and return the address
    /// it opened, which the host reaches once its port is forwarded.
    pub(crate) fn explore_page(&self, vision: &Vision) -> Result<String> {
        let sandbox = vision.sandbox();
        let action = sandbox.command(&self.reviewer_program.with_file_name("reviewer-control"));
        let page = self.scratch.open_explore_page(action, &self.space)?;
        sandbox.forward(loopback_port(&page)?).map_err(from_eyre)?;
        Ok(page)
    }

    /// Start the reviewer and its stand-in agent again, in a new tab, on the same repository
    /// and state; the agent runs the plugin's hooks as `agent_hooks` says, or as before. With
    /// `workspace`, both run in that workspace of the repository from now on (see
    /// [`Scratch::enter_workspace`]).
    pub(crate) fn reopen(
        &mut self,
        vision: &mut Vision,
        agent_hooks: Option<bool>,
        workspace: Option<&str>,
    ) -> Result<Screen> {
        // The reviewer first: the next one binds the same MCP port.
        close_terminal(
            vision,
            &self.terminals.reviewer,
            &self.terminals.reviewer_pane,
        )?;
        if let Some(workspace) = workspace {
            self.scratch.enter_workspace(workspace)?;
        }
        self.agent_hooks = agent_hooks.unwrap_or(self.agent_hooks);
        let (agent, agent_pane) = open_agent(
            vision,
            &self.scratch,
            &self.reviewer_program,
            self.agent_hooks,
            &self.events,
        )?;
        // The old agent once the new one holds the space: the app refuses to close a space's
        // last pane without a confirmation.
        close_terminal(vision, &self.terminals.agent, &self.terminals.agent_pane)?;
        self.terminals = open_reviewer(
            vision,
            &self.scratch,
            &self.reviewer_program,
            agent,
            agent_pane,
        )?;
        self.first_screen(vision)
    }

    /// Show the stand-in agent working without a turn, as an agent at work on what no prompt
    /// started, or idle again, and return the reviewer's screen once the app shows it so.
    pub(crate) fn agent_working(&self, vision: &mut Vision, working: bool) -> Result<Screen> {
        self.obey(&Role::agent(AGENT), &Command::ShowWorking { working })?;
        // Once its work ends, the app shows the agent `done` until its pane is seen, which the
        // reviewer takes as idle.
        let shown = |status: &str| {
            if working {
                status == "working"
            } else {
                matches!(status, "idle" | "done")
            }
        };
        let wanted = if working { "working" } else { "idle" };
        wait_app(
            vision,
            &format!("the app to show the agent {wanted}"),
            |listed| {
                terminal_entry(listed, &self.terminals.agent)
                    .and_then(|terminal| terminal["status"].as_str())
                    .is_some_and(shown)
            },
        )?;
        self.settle(vision)
    }

    fn obey(&self, role: &Role, command: &Command) -> Result<()> {
        let from = self.received()?.len();
        self.events
            .try_send(role, command, REACTION_GUARD)
            .map_err(|error| anyhow!(error))?;
        self.events
            .try_wait_until("the stand-in's answer", REACTION_GUARD, |received| {
                received[from..].iter().any(|reported| {
                    reported.from == *role
                        && matches!(&reported.event, Event::Obeyed { command: obeyed } if obeyed == command)
                })
            })
            .map_err(|error| anyhow!(error))?;
        Ok(())
    }

    fn received(&self) -> Result<Vec<Reported>> {
        self.events.try_received().map_err(|error| anyhow!(error))
    }
}

/// Open the stand-in agent in a new tab of the focused space, then the reviewer right of it.
fn open_terminals(
    vision: &Vision,
    scratch: &Scratch,
    reviewer: &Path,
    agent_hooks: bool,
    events: &Events,
) -> Result<Terminals> {
    let (agent, agent_pane) = open_agent(vision, scratch, reviewer, agent_hooks, events)?;
    open_reviewer(vision, scratch, reviewer, agent, agent_pane)
}

/// Open the stand-in agent in a new tab of the focused space, and return its terminal and its
/// pane once it connected to `events`.
fn open_agent(
    vision: &Vision,
    scratch: &Scratch,
    reviewer: &Path,
    agent_hooks: bool,
    events: &Events,
) -> Result<(String, String)> {
    let session = format!(
        "vision-{}",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    );
    let from = events.try_received().map_err(|error| anyhow!(error))?.len();
    let events_socket = vision.sandbox().inside().join(EVENTS_SOCKET);
    let agent = do_command(
        vision,
        "terminal.open",
        &json!({
            "argv": scratch.agent_command(reviewer, &session, agent_hooks, &events_socket),
            "folder": scratch.root(),
            "place": "tab",
            "name": "stand-in agent",
        }),
    )?;
    // The agent that connects from now on is the new one: one a reopen replaced keeps its
    // connection until its process ends.
    let role = Role::agent(AGENT);
    events
        .try_wait_until("the stand-in agent", REACTION_GUARD, |received| {
            received[from..].iter().any(|reported| {
                reported.from == role && matches!(reported.event, Event::Hello { .. })
            })
        })
        .map_err(|error| anyhow!(error))?;
    let terminal = string(&agent["terminal"])?;
    // A terminal opened in a new tab comes without its pane: the app lists it.
    let listed = vision.sandbox().herdr_app(&["ls"]).map_err(from_eyre)?;
    let pane = terminal_entry(&listed, &terminal)
        .map(|entry| string(&entry["pane"]))
        .transpose()?
        .with_context(|| format!("the app lists no terminal {terminal}: {listed:#}"))?;
    Ok((terminal, pane))
}

/// Open the reviewer right of the focused pane, following the agent's terminal `agent`
/// (`--focused-terminal`) and the app's focus, as the launcher mod opens it.
fn open_reviewer(
    vision: &Vision,
    scratch: &Scratch,
    reviewer: &Path,
    agent: String,
    agent_pane: String,
) -> Result<Terminals> {
    let app_home = vision
        .sandbox()
        .variable("HERDR_APP_HOME")
        .context("the sandbox names no HERDR_APP_HOME")?
        .to_string_lossy()
        .into_owned();
    let opened = do_command(
        vision,
        "terminal.open",
        &json!({
            "argv": Scratch::reviewer_command(reviewer, &agent, &app_home),
            "folder": scratch.root(),
            "place": "right",
            "name": "reviewer",
        }),
    )?;
    Ok(Terminals {
        agent,
        agent_pane,
        reviewer: string(&opened["terminal"])?,
        reviewer_pane: string(&opened["pane"])?,
    })
}

/// Close `pane`, and wait until the core reports its terminal `terminal` ended.
fn close_terminal(vision: &Vision, terminal: &str, pane: &str) -> Result<()> {
    do_command(vision, "pane.close", &json!({"pane": pane}))?;
    // `hc ls` lists live terminals only.
    wait_core(vision, &format!("{terminal} to end"), |listed| {
        !listed["terminals"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|entry| entry["address"] == terminal)
    })
}

/// `herdr-app do <command> <arguments>` in the session's app.
fn do_command(vision: &Vision, command: &str, arguments: &Value) -> Result<Value> {
    vision
        .sandbox()
        .herdr_app(&["do", command, &arguments.to_string()])
        .map_err(from_eyre)
}

/// The entry of the terminal `address` in `herdr-app ls`.
fn terminal_entry<'a>(listed: &'a Value, address: &str) -> Option<&'a Value> {
    listed["spaces"]
        .as_array()?
        .iter()
        .filter_map(|space| space["tabs"].as_array())
        .flatten()
        .filter_map(|tab| tab["terminals"].as_array())
        .flatten()
        .find(|terminal| terminal["terminal"] == address)
}

/// Wait until `herdr-app ls` satisfies `holds`.
fn wait_app(vision: &Vision, what: &str, holds: impl FnMut(&Value) -> bool) -> Result<()> {
    wait_listed(vision, || vision.sandbox().herdr_app(&["ls"]), what, holds)
}

/// Wait until `hc ls` satisfies `holds`.
fn wait_core(vision: &Vision, what: &str, holds: impl FnMut(&Value) -> bool) -> Result<()> {
    wait_listed(
        vision,
        || vision.sandbox().hc(&["ls", "--json"]),
        what,
        holds,
    )
}

/// Wait until what `list` lists satisfies `holds`: checked at once, then after each event of
/// the sandbox's core, which reports the terminals' statuses and ends.
fn wait_listed(
    vision: &Vision,
    mut list: impl FnMut() -> eyre::Result<Value>,
    what: &str,
    mut holds: impl FnMut(&Value) -> bool,
) -> Result<()> {
    let sandbox = vision.sandbox();
    // Subscribed before the first look: an event that comes between them still wakes the wait.
    let mut events = sandbox
        .command(&sandbox.programs().hc)
        .args(["events", "--json"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("cannot run hc events")?;
    let stdout = events.stdout.take().context("standard output is piped")?;
    let (sender, received) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if line.is_err() || sender.send(()).is_err() {
                return;
            }
        }
    });
    let deadline = Instant::now() + REACTION_GUARD;
    let waited = (|| loop {
        let listed = list().map_err(from_eyre)?;
        if holds(&listed) {
            return Ok(());
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if received.recv_timeout(remaining).is_err() {
            return Err(anyhow!(
                "waited {} ms for {what}; the app lists:\n{listed:#}",
                REACTION_GUARD.as_millis()
            ));
        }
    })();
    let _ = events.kill();
    let _ = events.wait();
    waited
}

/// The port of `address`, a page's address on the loopback interface
/// (`http://127.0.0.1:<port>/…`).
fn loopback_port(address: &str) -> Result<u16> {
    let port = address
        .strip_prefix("http://127.0.0.1:")
        .and_then(|rest| rest.split(['/', '?']).next())
        .with_context(|| format!("not an address on the loopback interface: {address}"))?;
    port.parse()
        .with_context(|| format!("no port in the address {address}"))
}

fn string(value: &Value) -> Result<String> {
    value
        .as_str()
        .map(str::to_owned)
        .with_context(|| format!("expected a string, got {value}"))
}

/// herdr-common's crates report with eyre: its report, chain included, as an anyhow error.
pub(crate) fn from_eyre(error: eyre::Report) -> anyhow::Error {
    anyhow::Error::from_boxed(error.into())
}

#[cfg(test)]
#[path = "session.tests.rs"]
mod tests;
