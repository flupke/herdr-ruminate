use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use agent_fork::{ProcessStamp, RunningFork, stop_recorded};
use agent_hooks::{AgentHooks, Heard};
use herdr_client::protocol::{Agent, AgentHost, PaneId};
use review_run_ahead::{ForkWaits, SwitchFailure, resume_block};

use crate::stream::Signal;

pub(crate) struct LiveFork {
    pub(crate) process: Arc<RunningFork>,
    pub(crate) answered: Arc<Signal>,
}

pub(crate) enum ForkProcess {
    Live(LiveFork),
    Recorded(ProcessStamp),
    Unknown,
}

impl ForkProcess {
    pub(crate) fn of(live: Option<LiveFork>, process: Option<ProcessStamp>) -> Self {
        match (live, process) {
            (Some(live), _) => Self::Live(live),
            (None, Some(process)) => Self::Recorded(process),
            (None, None) => Self::Unknown,
        }
    }

    pub(crate) fn stop(&self, session: &str) {
        match self {
            Self::Live(live) => live.process.terminate(),
            Self::Recorded(process) => {
                stop_recorded(*process, session);
            }
            Self::Unknown => {}
        }
    }

    fn wait_for_answer(&self, waits: ForkWaits) {
        if let Self::Live(live) = self {
            live.answered.wait(waits.answer);
        }
    }
}

pub(crate) struct AgentPane<'a> {
    pub(crate) host: &'a dyn AgentHost,
    pub(crate) pane: &'a PaneId,
}

impl AgentPane<'_> {
    fn agent(&self) -> Result<Agent, String> {
        self.host
            .get_agent(self.pane)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "the agent's pane is gone".to_owned())
    }

    fn ready_on(&self, session: &str) -> Option<Agent> {
        self.agent().ok().filter(|agent| {
            agent.agent_status.waits_for_prompt()
                && agent
                    .agent_session
                    .as_ref()
                    .is_some_and(|reported| reported.value == session)
        })
    }
}

pub(crate) struct Switch<'a> {
    pub(crate) pane: AgentPane<'a>,
    pub(crate) session: &'a str,
    pub(crate) fork: ForkProcess,
    pub(crate) waits: ForkWaits,
    pub(crate) hooks: Option<&'a AgentHooks>,
}

impl Switch<'_> {
    pub(crate) fn run(self) -> Result<Agent, SwitchFailure> {
        self.fork.wait_for_answer(self.waits);
        self.fork.stop(self.session);
        Resume {
            pane: self.pane,
            session: self.session,
            limit: self.waits.resume,
            poll: self.waits.resume_poll,
            hooks: self.hooks,
        }
        .run()
    }
}

pub(crate) struct Resume<'a> {
    pub(crate) pane: AgentPane<'a>,
    pub(crate) session: &'a str,
    pub(crate) limit: Duration,
    pub(crate) poll: Duration,
    pub(crate) hooks: Option<&'a AgentHooks>,
}

impl Resume<'_> {
    pub(crate) fn run(self) -> Result<Agent, SwitchFailure> {
        let hooks = self.hooks.ok_or_else(|| {
            SwitchFailure::untyped("the reviewer does not listen to the agent's hooks")
        })?;
        let agent = self.pane.agent().map_err(SwitchFailure::untyped)?;
        if !agent.agent_status.waits_for_prompt() {
            return Err(SwitchFailure::untyped("the agent in the pane is working"));
        }
        let Some(hooked) = hooks.hooked(self.pane.host, &agent) else {
            return Err(SwitchFailure::untyped(
                "the agent in the pane runs without the reviewer's Claude Code plugin",
            ));
        };
        let command = format!("/resume {}", self.session);
        let expectation = hooks.expect_resume(&hooked, self.session, &resume_block(&command));
        self.pane
            .host
            .submit_prompt(self.pane.pane, &command)
            .map_err(|error| SwitchFailure::untyped(error.to_string()))?;
        let deadline = Instant::now() + self.limit;
        let late = |what: &str| SwitchFailure {
            error: format!(
                "{what} the agent on the session {} within {} s",
                self.session,
                self.limit.as_secs_f64()
            ),
            typed: true,
        };
        match expectation.wait(deadline.saturating_duration_since(Instant::now())) {
            Some(Heard::Resumed) => {}
            // Claude Code submits no prompt for a command of its own: a prompt means that text
            // left in its input box joined the command.
            Some(Heard::Blocked { prompt }) => {
                return Err(SwitchFailure::blocked(&prompt, &command));
            }
            Some(Heard::Submitted | Heard::Joined { .. } | Heard::Stopped) | None => {
                return Err(late("the agent's hook did not report"));
            }
        }
        drop(expectation);
        // Herdr hears of the session from its own hook, which Claude Code runs beside ours: Herdr
        // may not know it yet, and sends no event once it does.
        loop {
            if let Some(agent) = self.pane.ready_on(self.session) {
                return Ok(agent);
            }
            if Instant::now() >= deadline {
                return Err(late("Herdr did not report"));
            }
            thread::sleep(self.poll);
        }
    }
}
