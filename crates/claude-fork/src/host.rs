//! [`ClaudeForks`]: run-ahead's forks of a Claude Code agent in a Herdr pane.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use agent_fork::{Launcher, Wrapper};
use agent_hooks::AgentHooks;
use herdr_client::protocol::{Agent, AgentHost, PaneId, PaneProcess};
use review_run_ahead::{
    ForkEnd, ForkHost, ForkPoint, ForkStart, ForkTrace, ForkWaits, HostFields, OwnedFork,
    PaneWatch, RunAheadLog, StatusReport, SwitchFailure, SwitchTo,
};

use crate::fields;
use crate::guard::SUBMITS;
use crate::pane::PaneClaude;
use crate::settings::{AgentSettings, SettingsFile};
use crate::stream::StreamTally;
use crate::switch::{AgentPane, ForkProcess, LiveFork, Resume, Switch};
use crate::transcripts::Transcripts;

/// The programs forks need from the reviewer's installation: `reviewer-control`, whose
/// `fork-exec` ties a fork's life to the reviewer's and whose `fork-guard` is the forks' hook.
#[derive(Clone, Debug)]
pub struct ForkTools {
    pub control: PathBuf,
}

impl ForkTools {
    /// The `reviewer-control` installed beside the running program.
    pub fn beside_current_exe() -> std::io::Result<Self> {
        Ok(Self {
            control: std::env::current_exe()?.with_file_name("reviewer-control"),
        })
    }

    /// What starts a process that must not outlive the reviewer: `fork-exec`.
    pub fn wrapper(&self) -> Wrapper {
        Wrapper {
            program: self.control.clone(),
            arguments: vec!["fork-exec".into()],
        }
    }

    /// The settings of a fork of an agent with the settings `agent`: those, with the guard as
    /// their hook.
    fn settings(&self, agent: &AgentSettings) -> String {
        let control = self.control.to_string_lossy().replace('\'', r"'\''");
        agent.with_guard(&format!("'{control}' fork-guard"))
    }
}

/// Forks of a Claude Code agent's session, started from the [`Launcher`]'s thread and followed
/// until they end. Dropping it sends every fork still running its parent-death signal.
pub struct ClaudeForks {
    host: Arc<dyn AgentHost>,
    tools: ForkTools,
    launcher: Launcher,
    log: RunAheadLog,
    waits: ForkWaits,
    /// What the hooks of the agents say, when the reviewer listens to them.
    hooks: Option<Arc<AgentHooks>>,
    /// The forks that run, by session.
    live: Arc<Mutex<HashMap<String, LiveFork>>>,
    /// The discards under way, which stop a fork and delete its transcript.
    discards: Mutex<Vec<thread::JoinHandle<()>>>,
}

impl ClaudeForks {
    /// Forks of the agents `herdr` reports, started through `tools`, logged to `log`, waiting
    /// on Herdr and on the forks as `waits` say, and on what the agents' hooks tell the reviewer
    /// through `hooks`, or else why the reviewer does not listen to them.
    pub fn new(
        host: Arc<dyn AgentHost>,
        tools: ForkTools,
        log: RunAheadLog,
        waits: ForkWaits,
        hooks: Result<Arc<AgentHooks>, String>,
    ) -> Self {
        let (hooks, unheard) = match hooks {
            Ok(hooks) => (Some(hooks), None),
            Err(error) => (None, Some(error)),
        };
        let forks = Self {
            launcher: Launcher::start(tools.wrapper()),
            host,
            tools,
            log,
            waits,
            hooks,
            live: Arc::default(),
            discards: Mutex::default(),
        };
        if let Some(error) = unheard {
            forks.log(&error);
        }
        forks
    }

    fn live(&self) -> std::sync::MutexGuard<'_, HashMap<String, LiveFork>> {
        self.live.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl ClaudeForks {
    /// The hooks the reviewer hears `agent` through, and its session, when run-ahead can fork
    /// the agent if it runs them; otherwise why not.
    fn forkable<'a>(&'a self, agent: &'a Agent) -> Result<(&'a AgentHooks, &'a str), String> {
        if !cfg!(target_os = "linux") {
            // Only Linux has the parent-death signal that keeps a fork from outliving the
            // reviewer when the reviewer runs outside a terminal.
            return Err("run-ahead forks only on Linux".into());
        }
        if !runs_claude(agent) {
            return Err(format!(
                "run-ahead forks Claude Code, and the agent's pane runs {}",
                agent
                    .agent
                    .as_deref()
                    .unwrap_or("an agent Herdr does not name")
            ));
        }
        let hooks = self
            .hooks
            .as_deref()
            .ok_or("the reviewer hears no agent's hooks: its log says why")?;
        let session = agent
            .agent_session
            .as_ref()
            .ok_or("Herdr reports no session for the agent")?;
        Ok((hooks, &session.value))
    }
}

impl Drop for ClaudeForks {
    /// Lets the discards under way finish, so no transcript of a discarded fork is left.
    fn drop(&mut self) {
        let discards =
            std::mem::take(&mut *self.discards.lock().unwrap_or_else(PoisonError::into_inner));
        for discard in discards {
            let _ = discard.join();
        }
    }
}

/// The Claude Code process of `pane`.
fn pane_claude(host: &dyn AgentHost, pane: &PaneId) -> Result<PaneClaude, String> {
    let pid = host
        .pane_process_info(pane)
        .map_err(|error| error.to_string())?
        .foreground_processes
        .iter()
        .find(|process| is_claude(process))
        .map(|process| process.pid)
        .ok_or("no Claude Code process runs in the agent's pane")?;
    PaneClaude::read(pid)
}

/// Waits up to `wait` for the transcript of the Claude Code agent of `pane` to hold the end of
/// its turn; returns at once for another agent, or one whose session, process or transcript
/// cannot be read, which run-ahead does not fork.
fn wait_for_turn_end(host: &dyn AgentHost, pane: &PaneId, wait: Duration) {
    let Some(session) = host
        .get_agent(pane)
        .ok()
        .flatten()
        .filter(runs_claude)
        .and_then(|agent| agent.agent_session)
    else {
        return;
    };
    if let Ok(claude) = pane_claude(host, pane) {
        claude.transcripts().turn_end(&session.value, wait);
    }
}

/// Whether Herdr reports `agent` as Claude Code.
fn runs_claude(agent: &Agent) -> bool {
    agent.agent.as_deref() == Some("claude")
}

/// Whether a process of the pane is Claude Code.
fn is_claude(process: &PaneProcess) -> bool {
    process.name == "claude"
        || process
            .argv
            .as_ref()
            .and_then(|argv| argv.first())
            .is_some_and(|program| program.rsplit('/').next() == Some("claude"))
}

impl ForkHost for ClaudeForks {
    fn watch(&self, pane: &PaneId, report: StatusReport) -> PaneWatch {
        // The agent is idle for run-ahead once its transcript holds the end of its turn, which
        // Claude Code writes after it reports the turn ended to herdr-core: the watch's thread
        // waits for it, so that no fork starts from an older entry.
        let (host, watched, wait) = (Arc::clone(&self.host), pane.clone(), self.waits.transcript);
        let report: StatusReport = Box::new(move |status| {
            if status.waits_for_prompt() {
                wait_for_turn_end(&*host, &watched, wait);
            }
            report(status);
        });
        PaneWatch::host(&self.host, pane, self.waits.resubscribe, report)
    }

    fn session(&self, agent: &Agent) -> Option<String> {
        agent
            .agent_session
            .as_ref()
            .map(|session| session.value.clone())
    }

    fn unhooked(&self, agent: &Agent) -> Option<String> {
        // An agent run-ahead cannot fork for another reason has that reason, which `point`
        // gives.
        self.forkable(agent)
            .is_ok_and(|(hooks, _)| hooks.hooked(&*self.host, agent).is_none())
            .then(|| "the agent runs without the reviewer's Claude Code plugin: restart it".into())
    }

    fn can_move(&self, agent: &Agent) -> bool {
        self.forkable(agent)
            .is_ok_and(|(hooks, _)| hooks.hooked(&*self.host, agent).is_some())
    }

    fn unforkable(&self, agent: &Agent) -> Option<String> {
        self.forkable(agent).err()
    }

    fn point(&self, agent: &Agent) -> Result<ForkPoint, String> {
        let session = self.forkable(agent)?.1.to_owned();
        let pane = pane_claude(&*self.host, &agent.pane_id)?;
        let transcripts = pane.transcripts();
        let last = transcripts
            .last_entry(&session)
            .ok_or_else(|| format!("the session {session} has no transcript"))?;
        if !last.ends_turn {
            self.log(&format!(
                "the transcript of {session} shows no end of the agent's turn: forking from its \
                 last entry"
            ));
        }
        Ok(ForkPoint {
            session,
            entry: last.entry,
            model: last.model.or(pane.model),
            host: fields::point(transcripts.root(), &pane.command, &pane.settings),
        })
    }

    fn last_entry(&self, point: &ForkPoint) -> Option<String> {
        Transcripts::at(fields::transcripts(&point.host)?)
            .last_entry(&point.session)?
            .entry
    }

    fn create(&self, point: &ForkPoint) -> Result<OwnedFork, String> {
        let transcripts =
            fields::transcripts(&point.host).ok_or("the point names no transcript directory")?;
        Ok(OwnedFork {
            session: uuid::Uuid::new_v4().to_string(),
            host: fields::fork(transcripts),
        })
    }

    fn start(
        &self,
        start: ForkStart<'_>,
        prompt: String,
        ended: Box<dyn FnOnce(ForkEnd) + Send>,
    ) -> Result<HostFields, String> {
        let point = start.point;
        let mut command =
            fields::command(&point.host).ok_or("the point names no Claude Code command")?;
        let settings = self
            .tools
            .settings(&fields::settings(&point.host).unwrap_or_default());
        let settings = SettingsFile::write(start.fork.session, &settings)
            .map_err(|error| format!("cannot write the fork's settings: {error}"))?;
        command.arguments.extend(
            [
                "-p",
                "--resume",
                &point.session,
                "--fork-session",
                "--session-id",
                start.fork.session,
                "--output-format",
                "stream-json",
                "--verbose",
                "--settings",
                &settings.path().to_string_lossy(),
                "--allowedTools",
            ]
            .into_iter()
            .chain(SUBMITS.iter().copied())
            .map(OsString::from),
        );
        if let Some(model) = &point.model {
            command.arguments.extend(["--model".into(), model.into()]);
        }
        let program = command.program.display().to_string();
        let (live, session) = (Arc::clone(&self.live), start.fork.session.to_owned());
        let ended = Box::new(move |end| {
            live.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&session);
            // The fork read its settings when it started; they go with it.
            drop(settings);
            ended(end);
        });
        let tally = StreamTally::new(ended);
        let (answered, unguarded) = (tally.submit_answered(), tally.unguarded());
        let fork = Arc::new(
            self.launcher
                .spawn(command, prompt, Box::new(tally))
                .map_err(|error| format!("cannot start {program}: {error}"))?,
        );
        unguarded.arm(Box::new({
            let fork = Arc::clone(&fork);
            move || fork.terminate()
        }));
        let stamp = fork.stamp();
        self.live().insert(
            start.fork.session.to_owned(),
            LiveFork {
                process: fork,
                answered,
            },
        );
        Ok(fields::started(stamp))
    }

    fn discard(&self, fork: ForkTrace<'_>, done: Box<dyn FnOnce() + Send>) {
        let running = self.live().remove(fork.session);
        let (session, process) = (fork.session.to_owned(), fields::process(fork.host));
        let transcripts = fields::transcripts(fork.host).map(Transcripts::at);
        let discard = thread::spawn(move || {
            ForkProcess::of(running, process).stop(&session);
            if let Some(transcripts) = transcripts {
                transcripts.delete(&session);
            }
            done();
        });
        let mut discards = self.discards.lock().unwrap_or_else(PoisonError::into_inner);
        discards.retain(|discard| !discard.is_finished());
        discards.push(discard);
    }

    fn switch(
        &self,
        switch: SwitchTo<'_>,
        done: Box<dyn FnOnce(Result<Agent, SwitchFailure>) + Send>,
    ) {
        let fork = ForkProcess::of(
            self.live().remove(switch.fork.session),
            fields::process(switch.fork.host),
        );
        let (host, pane, session) = (
            Arc::clone(&self.host),
            switch.pane.clone(),
            switch.fork.session.to_owned(),
        );
        let (waits, hooks) = (self.waits, self.hooks.clone());
        thread::spawn(move || {
            let switch = Switch {
                pane: AgentPane {
                    host: &*host,
                    pane: &pane,
                },
                session: &session,
                fork,
                waits,
                hooks: hooks.as_deref(),
            };
            done(switch.run());
        });
    }

    fn resume(
        &self,
        pane: &PaneId,
        _from: ForkTrace<'_>,
        session: &str,
        done: Box<dyn FnOnce(Result<Agent, SwitchFailure>) + Send>,
    ) {
        let (host, pane, session) = (Arc::clone(&self.host), pane.clone(), session.to_owned());
        let (waits, hooks) = (self.waits, self.hooks.clone());
        thread::spawn(move || {
            let resume = Resume {
                pane: AgentPane {
                    host: &*host,
                    pane: &pane,
                },
                session: &session,
                limit: waits.settle,
                poll: waits.resume_poll,
                hooks: hooks.as_deref(),
            };
            done(resume.run());
        });
    }

    fn log(&self, line: &str) {
        self.log.write(line);
    }
}
