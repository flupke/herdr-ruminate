use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, ensure};
use app_sandbox::{AppSandbox, Isolation, Options, Programs};
use fake_agent::{Command, Event, Events, Reported, Role};
use notify::Watcher as _;
use review_repository::repository::RepoType;
use serde_json::{Value, json};
use vision_mcp::{Input, Screen, Vision};

use super::agent::ScriptedAgent;
use super::forks;
use super::scratch::{Scratch, Seed};

const REACTION_GUARD: Duration = Duration::from_secs(30);

const DISPLAY_SIZE: (u16, u16) = (1600, 1000);

pub(crate) const AGENT: &str = "agent";

// Linux limits a Unix socket's path to 107 bytes.
const EVENTS_SOCKET: &str = "fake-agent.sock";

// Claude Code names an MCP tool `mcp__<server>__<tool>`.
const MCP_TOOL_PREFIX: &str = "mcp__herdr_reviewer__";

pub(crate) struct StartOptions {
    pub(crate) repository: RepoType,
    pub(crate) seed: Seed,
    pub(crate) agent_hooks: bool,
}

struct Terminals {
    agent: String,
    agent_pane: String,
    reviewer: String,
    reviewer_pane: String,
}

pub(crate) struct Session {
    space: String,
    terminals: Terminals,
    events: Events,
    scratch: Scratch,
    reviewer_program: PathBuf,
    agent_hooks: bool,
    agent: ScriptedAgent,
}

impl Session {
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
                ..Options::default()
            },
        )
        .map_err(from_eyre)?;
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
        // herdr-app opens a new space with a shell in its first tab.
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

    fn first_screen(&self, vision: &Vision) -> Result<Screen> {
        wait_app(vision, "the app to see the fake agent's status", |listed| {
            terminal_entry(listed, &self.terminals.agent).is_some_and(|t| !t["status"].is_null())
        })?;
        vision
            .wait_for(
                "Explore",
                false,
                REACTION_GUARD,
                Some(&self.terminals.reviewer),
            )
            .map_err(|error| anyhow!("the reviewer did not start: {error:#}"))
    }

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

    // herdr-common's vision core sends the acknowledgement request to the focused terminal.
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

    pub(crate) fn forks(&self, count: Option<usize>, guard: Duration) -> Result<Value> {
        if let Some(count) = count {
            self.events
                .try_wait_until(&format!("{count} forks"), guard, |received| {
                    forks::forks(received).len() >= count
                })
                .map_err(|error| anyhow!(error))?;
        }
        let received = self.received()?;
        Ok(json!({
            "forks": forks::forks(&received),
            "agent": forks::agent_sessions(&received),
            "log": fs::read_to_string(self.scratch.run_ahead_log()).unwrap_or_default(),
        }))
    }

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
        let ended = format!("({session}) ended");
        wait_logged(&self.scratch.run_ahead_log(), "the fork's end", |log| {
            log.contains(&ended)
        })?;
        self.settle(vision)
    }

    fn fork(&self, session: &str) -> Result<forks::Fork> {
        forks::forks(&self.received()?)
            .into_iter()
            .find(|fork| fork.session == session)
            .with_context(|| format!("no fork has the session {session}"))
    }

    pub(crate) fn switch_unheard(&self, heard_after: Option<Duration>) -> Result<()> {
        let agent = self.scratch.agent_directory();
        fs::create_dir_all(agent.path())?;
        let late = heard_after.map_or_else(String::new, |late| late.as_millis().to_string());
        fs::write(agent.unreported_resume(), late)?;
        Ok(())
    }

    pub(crate) fn agent_prompt(&self, vision: &mut Vision, text: &str) -> Result<Screen> {
        ensure!(
            !text.trim().is_empty(),
            "the agent's prompt is empty: the agent would not read it"
        );
        let from = self.received()?.len();
        let agent = Role::agent(AGENT);
        // `hc send` pastes the text and presses Enter without moving the focus.
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

    pub(crate) fn explore_page(&self, vision: &Vision) -> Result<String> {
        let sandbox = vision.sandbox();
        let action = sandbox.command(&self.reviewer_program.with_file_name("reviewer-control"));
        let listed = sandbox.herdr_app(&["ls"]).map_err(from_eyre)?;
        let tab = tab_of(&listed, &self.terminals.reviewer)
            .ok_or_else(|| anyhow!("the app lists no tab of the reviewer"))?;
        let page = self.scratch.open_explore_page(action, &self.space, tab)?;
        sandbox.forward(loopback_port(&page)?).map_err(from_eyre)?;
        Ok(page)
    }

    pub(crate) fn reopen(
        &mut self,
        vision: &mut Vision,
        agent_hooks: Option<bool>,
        workspace: Option<&str>,
    ) -> Result<Screen> {
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
        // herdr-app refuses to close a space's last pane without a confirmation.
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

    pub(crate) fn agent_working(&self, vision: &mut Vision, working: bool) -> Result<Screen> {
        self.obey(&Role::agent(AGENT), &Command::ShowWorking { working })?;
        // herdr-app shows an agent `done` once its work ends, until its pane is seen.
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
            .try_wait_until("the fake agent's answer", REACTION_GUARD, |received| {
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
            "name": "fake agent",
        }),
    )?;
    let role = Role::agent(AGENT);
    events
        .try_wait_until("the fake agent", REACTION_GUARD, |received| {
            received[from..].iter().any(|reported| {
                reported.from == role && matches!(reported.event, Event::Hello { .. })
            })
        })
        .map_err(|error| anyhow!(error))?;
    let terminal = string(&agent["terminal"])?;
    // herdr-app answers a terminal opened in a new tab without its pane: `herdr-app ls` lists it.
    let listed = vision.sandbox().herdr_app(&["ls"]).map_err(from_eyre)?;
    let pane = terminal_entry(&listed, &terminal)
        .map(|entry| string(&entry["pane"]))
        .transpose()?
        .with_context(|| format!("the app lists no terminal {terminal}: {listed:#}"))?;
    Ok((terminal, pane))
}

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

fn close_terminal(vision: &Vision, terminal: &str, pane: &str) -> Result<()> {
    do_command(vision, "pane.close", &json!({"pane": pane}))?;
    wait_terminal_ended(vision, terminal)
}

fn wait_terminal_ended(vision: &Vision, terminal: &str) -> Result<()> {
    // `hc ls` lists live terminals only.
    wait_core(vision, &format!("{terminal} to end"), |listed| {
        !listed["terminals"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|entry| entry["address"] == terminal)
    })
}

fn do_command(vision: &Vision, command: &str, arguments: &Value) -> Result<Value> {
    vision
        .sandbox()
        .herdr_app(&["do", command, &arguments.to_string()])
        .map_err(from_eyre)
}

fn listed_tabs(listed: &Value) -> impl Iterator<Item = &Value> {
    listed["spaces"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|space| space["tabs"].as_array())
        .flatten()
}

fn tab_terminals(tab: &Value) -> impl Iterator<Item = &Value> {
    tab["terminals"].as_array().into_iter().flatten()
}

fn terminal_entry<'a>(listed: &'a Value, address: &str) -> Option<&'a Value> {
    listed_tabs(listed)
        .flat_map(tab_terminals)
        .find(|terminal| terminal["terminal"] == address)
}

fn tab_of<'a>(listed: &'a Value, address: &str) -> Option<&'a str> {
    listed_tabs(listed)
        .find(|tab| tab_terminals(tab).any(|terminal| terminal["terminal"] == address))?["id"]
        .as_str()
}

fn wait_logged(log: &Path, what: &str, holds: impl Fn(&str) -> bool) -> Result<()> {
    let (written, writes) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if event.is_ok_and(|event| {
            matches!(
                event.kind,
                notify::EventKind::Create(_) | notify::EventKind::Modify(_)
            )
        }) {
            let _ = written.send(());
        }
    })?;
    let directory = log.parent().context("the log is in no directory")?;
    watcher.watch(directory, notify::RecursiveMode::NonRecursive)?;
    let deadline = Instant::now() + REACTION_GUARD;
    loop {
        let logged = fs::read_to_string(log).unwrap_or_default();
        if holds(&logged) {
            return Ok(());
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() || writes.recv_timeout(remaining).is_err() {
            return Err(anyhow!(
                "waited {} ms for the reviewer to log {what}; it logged:\n{logged}",
                REACTION_GUARD.as_millis()
            ));
        }
    }
}

fn wait_app(vision: &Vision, what: &str, holds: impl FnMut(&Value) -> bool) -> Result<()> {
    wait_listed(vision, || vision.sandbox().herdr_app(&["ls"]), what, holds)
}

fn wait_core(vision: &Vision, what: &str, holds: impl FnMut(&Value) -> bool) -> Result<()> {
    wait_listed(
        vision,
        || vision.sandbox().hc(&["ls", "--json"]),
        what,
        holds,
    )
}

fn wait_listed(
    vision: &Vision,
    mut list: impl FnMut() -> eyre::Result<Value>,
    what: &str,
    mut holds: impl FnMut(&Value) -> bool,
) -> Result<()> {
    let sandbox = vision.sandbox();
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

pub(crate) fn from_eyre(error: eyre::Report) -> anyhow::Error {
    anyhow::Error::from_boxed(error.into())
}

#[cfg(test)]
#[path = "session.tests.rs"]
mod tests;
