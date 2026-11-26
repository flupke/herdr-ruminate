use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use agent_hooks::{AgentHooks, Expectation, Heard, HookedAgent};
use codex_daemon::{Daemon, SessionEvent, SessionStatus};
use herdr_client::protocol::{Agent, AgentHost, PaneId, SessionReport};
use review_run_ahead::{RunAheadLog, SwitchFailure, resume_block};

const JOIN_LIMIT: Duration = Duration::from_secs(10);

const HERDR_CODEX: &str = "herdr:codex";

pub(crate) struct Resume<'a> {
    pub(crate) host: &'a dyn AgentHost,
    pub(crate) pane: &'a PaneId,
    pub(crate) hooks: Option<&'a AgentHooks>,
    pub(crate) daemon: Daemon,
    pub(crate) session: &'a str,
    pub(crate) maybe_on: Option<&'a str>,
    pub(crate) limit: Duration,
    pub(crate) poll: Duration,
    pub(crate) retry: Duration,
    pub(crate) log: &'a RunAheadLog,
}

#[derive(Default)]
struct Joined {
    resumed: bool,
    status: Option<SessionStatus>,
}

impl Joined {
    fn hear(&mut self, events: &Receiver<SessionEvent>) {
        for event in events.try_iter() {
            match event {
                SessionEvent::Resumed => self.resumed = true,
                SessionEvent::Joined(status) => self.status = status.or(self.status),
                SessionEvent::Status(status) => self.status = Some(status),
                SessionEvent::TurnStarted { .. } => self.status = Some(SessionStatus::Active),
                _ => {}
            }
        }
    }
}

impl Resume<'_> {
    pub(crate) fn run(self) -> Result<Agent, SwitchFailure> {
        let hooks = self.hooks.ok_or_else(|| {
            SwitchFailure::untyped("the reviewer does not listen to the agent's hooks")
        })?;
        let agent = self
            .host
            .get_agent(self.pane)
            .map_err(|error| SwitchFailure::untyped(error.to_string()))?
            .ok_or_else(|| SwitchFailure::untyped("the agent's pane is gone"))?;
        if !agent.agent_status.waits_for_prompt() {
            return Err(SwitchFailure::untyped("the agent in the pane is working"));
        }
        let Some(hooked) = hooks.hooked(self.host, &agent) else {
            return Err(SwitchFailure::untyped(
                "the agent in the pane runs without the reviewer's Codex plugin",
            ));
        };
        let (heard, events) = mpsc::channel();
        let _follow = self.daemon.follow(self.session, self.retry, move |event| {
            let _ = heard.send(event);
        });
        let joined = self.join(&events, Instant::now() + JOIN_LIMIT)?;
        let command = format!("/resume {}", self.session);
        let block = resume_block(&command);
        // Codex joins text left in its input box and the typed `/resume` into one prompt, which
        // runs on the session the agent is on.
        let expectations: Vec<Expectation> = std::iter::once(hooked)
            .chain(self.maybe_on.map(|session| HookedAgent::Codex {
                session: session.to_owned(),
            }))
            .map(|on| hooks.expect_resume(&on, self.session, &block))
            .collect();
        self.host
            .submit_prompt(self.pane, &command)
            .map_err(|error| SwitchFailure::untyped(error.to_string()))?;
        let deadline = Instant::now() + self.limit;
        self.wait_until_moved(&expectations, &events, joined, &command, deadline)?;
        drop(expectations);
        hooks.name_session(self.pane, self.session);
        if agent.agent_session.is_some() {
            self.report_to_host();
        }
        Ok(self
            .host
            .get_agent(self.pane)
            .ok()
            .flatten()
            .unwrap_or(agent))
    }

    fn join(
        &self,
        events: &Receiver<SessionEvent>,
        deadline: Instant,
    ) -> Result<Joined, SwitchFailure> {
        loop {
            match events.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(SessionEvent::Joined(status)) => {
                    return Ok(Joined {
                        resumed: false,
                        status,
                    });
                }
                Ok(SessionEvent::Refused(error)) => {
                    return Err(SwitchFailure::untyped(format!(
                        "the Codex daemon cannot join the session {}: {error}",
                        self.session
                    )));
                }
                Ok(_) => {}
                Err(_) => {
                    return Err(SwitchFailure::untyped(format!(
                        "the reviewer could not join the session {} on the Codex daemon",
                        self.session
                    )));
                }
            }
        }
    }

    fn wait_until_moved(
        &self,
        expectations: &[Expectation],
        events: &Receiver<SessionEvent>,
        mut joined: Joined,
        command: &str,
        deadline: Instant,
    ) -> Result<(), SwitchFailure> {
        let slice = self.poll / u32::try_from(expectations.len()).unwrap_or(1).max(1);
        let blocked = |limit: Duration| {
            expectations
                .iter()
                .find_map(|expectation| match expectation.wait(limit) {
                    Some(Heard::Blocked { prompt }) => {
                        Some(SwitchFailure::blocked(&prompt, command))
                    }
                    _ => None,
                })
        };
        loop {
            if let Some(failure) = blocked(Duration::ZERO) {
                return Err(failure);
            }
            joined.hear(events);
            if joined.resumed && joined.status == Some(SessionStatus::Idle) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(SwitchFailure {
                    error: format!(
                        "the Codex daemon did not report the agent on the session {} within {} s",
                        self.session,
                        self.limit.as_secs_f64()
                    ),
                    typed: true,
                });
            }
            if let Some(failure) = blocked(slice) {
                return Err(failure);
            }
        }
    }

    // Herdr's own Codex hook reports a resumed session only at the agent's next turn.
    fn report_to_host(&self) {
        let seq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |now| u64::try_from(now.as_nanos()).unwrap_or(u64::MAX));
        let reported = self.host.report_agent_session(
            self.pane,
            &SessionReport {
                source: HERDR_CODEX,
                agent: "codex",
                seq,
                session: self.session,
                start: "resume",
            },
        );
        if let Err(error) = reported {
            self.log.write(&format!(
                "the host was not told that the agent of {} runs the session {}: {error}",
                self.pane.0, self.session
            ));
        }
    }
}
