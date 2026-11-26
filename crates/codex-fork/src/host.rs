use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

use agent_hooks::AgentHooks;
use codex_daemon::{Daemon, NoDaemon, Rpc};
use herdr_client::protocol::{Agent, AgentHost, AgentPort, PaneId};
use review_run_ahead::{
    ForkEnd, ForkHost, ForkPoint, ForkStart, ForkTrace, ForkWaits, HostFields, OwnedFork,
    PaneWatch, RunAheadLog, StatusReport, SwitchFailure, SwitchTo,
};
use serde_json::{Value, json};

use crate::fields;
use crate::live::{LiveFork, TurnWatch};
use crate::resume::Resume;

const REQUEST_LIMIT: Duration = Duration::from_secs(10);

pub struct CodexForks {
    host: Arc<dyn AgentHost>,
    hooks: Option<Arc<AgentHooks>>,
    log: RunAheadLog,
    waits: ForkWaits,
    live: Mutex<HashMap<String, LiveFork>>,
    threads: Mutex<Vec<thread::JoinHandle<()>>>,
}

impl CodexForks {
    pub fn new(
        host: Arc<dyn AgentHost>,
        log: RunAheadLog,
        waits: ForkWaits,
        hooks: Option<Arc<AgentHooks>>,
    ) -> Self {
        Self {
            host,
            hooks,
            log,
            waits,
            live: Mutex::default(),
            threads: Mutex::default(),
        }
    }

    fn live(&self) -> MutexGuard<'_, HashMap<String, LiveFork>> {
        self.live.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn mover(&self) -> Mover {
        Mover {
            host: Arc::clone(&self.host),
            hooks: self.hooks.clone(),
            log: self.log.clone(),
            waits: self.waits,
        }
    }

    fn in_background(&self, work: impl FnOnce() + Send + 'static) {
        let mut threads = self.threads.lock().unwrap_or_else(PoisonError::into_inner);
        threads.retain(|thread| !thread.is_finished());
        threads.push(thread::spawn(work));
    }

    fn daemon_of(&self, agent: &Agent) -> Result<Daemon, NoDaemon> {
        let processes = self
            .host
            .pane_process_info(&agent.pane_id)
            .map_err(|_| NoDaemon::Unreadable("pane"))?;
        Daemon::of_pane(&processes.foreground_processes)
    }

    fn guarded(daemon: &Daemon) -> Result<bool, String> {
        let rpc = connect(daemon)?;
        let listed = rpc.request("hooks/list", &json!({}), REQUEST_LIMIT)?;
        let hooks = listed
            .get("data")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|directory| directory.get("hooks").and_then(Value::as_array))
            .flatten();
        Ok(hooks.into_iter().any(|hook| {
            let field = |name: &str| hook.get(name).and_then(Value::as_str);
            field("pluginId") == Some(codex_hooks::PLUGIN)
                && field("eventName") == Some("preToolUse")
                && hook.get("enabled").and_then(Value::as_bool) == Some(true)
                && matches!(field("trustStatus"), Some("trusted" | "managed"))
        }))
    }

    fn last_turn(rpc: &Rpc, session: &str) -> Result<Option<String>, String> {
        Ok(latest_turn(rpc, session)?
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_owned))
    }
}

impl Drop for CodexForks {
    fn drop(&mut self) {
        let threads =
            std::mem::take(&mut *self.threads.lock().unwrap_or_else(PoisonError::into_inner));
        for thread in threads {
            let _ = thread.join();
        }
    }
}

fn connect(daemon: &Daemon) -> Result<Rpc, String> {
    daemon.connect(|_, _| {})
}

// Codex refuses to interrupt a turn that ended, which changes nothing.
fn interrupt(rpc: &Rpc, session: &str, turn: &str) {
    let _ = rpc.request(
        "turn/interrupt",
        &json!({"threadId": session, "turnId": turn}),
        REQUEST_LIMIT,
    );
}

fn update_settings(rpc: &Rpc, session: &str, settings: &Value) -> Result<(), String> {
    let mut params = settings.clone();
    params["threadId"] = session.into();
    rpc.request("thread/settings/update", &params, REQUEST_LIMIT)
        .map(|_| ())
}

fn end_turn(live: Option<LiveFork>, daemon: &Daemon, fork: &OwnedFork, waits: ForkWaits) -> bool {
    let limit = waits.answer;
    let Some(live) = live else {
        let Ok(rpc) = connect(daemon) else {
            return false;
        };
        if let Some(turn) = fields::turn(&fork.host) {
            interrupt(&rpc, &fork.session, &turn);
        }
        let deadline = std::time::Instant::now() + limit;
        loop {
            if last_turn_ended(&rpc, &fork.session) {
                return true;
            }
            if std::time::Instant::now() >= deadline {
                return false;
            }
            thread::sleep(waits.resume_poll);
        }
    };
    live.turn.wait_over(limit) || {
        if let Some(turn) = live.turn.id() {
            interrupt(&live.rpc, &fork.session, &turn);
        }
        live.turn.wait_over(limit)
    }
}

fn latest_turn(rpc: &Rpc, session: &str) -> Result<Value, String> {
    let mut turns = rpc.request(
        "thread/turns/list",
        &json!({"threadId": session, "limit": 1, "sortDirection": "desc"}),
        REQUEST_LIMIT,
    )?;
    Ok(turns
        .pointer_mut("/data/0")
        .map(Value::take)
        .unwrap_or_default())
}

fn last_turn_ended(rpc: &Rpc, session: &str) -> bool {
    latest_turn(rpc, session).is_ok_and(|turn| {
        turn.get("status")
            .and_then(Value::as_str)
            .is_none_or(|status| status != "inProgress")
    })
}

fn no_daemon() -> SwitchFailure {
    SwitchFailure::untyped("the fork's record names no Codex daemon")
}

struct Mover {
    host: Arc<dyn AgentHost>,
    hooks: Option<Arc<AgentHooks>>,
    log: RunAheadLog,
    waits: ForkWaits,
}

impl Mover {
    // Codex keeps a session's settings when another client resumes it.
    fn hand_over(&self, daemon: &Daemon, fork: ForkTrace<'_>) {
        let session = fork.session;
        if let Some(hooks) = &self.hooks
            && let Err(error) = hooks.root().unmark_codex_fork(session)
        {
            self.log.write(&format!(
                "the fork {session}'s guard was not lifted: {error}"
            ));
        }
        let given_back = fields::settings(fork.host)
            .ok_or_else(|| "its record keeps no settings of the agent's".to_owned())
            .and_then(|settings| update_settings(&connect(daemon)?, session, &settings));
        if let Err(error) = given_back {
            self.log.write(&format!(
                "the fork {session}'s session did not take back the agent's settings: {error}"
            ));
        }
    }

    fn resume(
        &self,
        pane: &PaneId,
        daemon: Daemon,
        session: &str,
        maybe_on: Option<&str>,
        limit: Duration,
    ) -> Result<Agent, SwitchFailure> {
        Resume {
            host: &*self.host,
            pane,
            hooks: self.hooks.as_deref(),
            daemon,
            session,
            maybe_on,
            limit,
            poll: self.waits.resume_poll,
            retry: self.waits.resubscribe,
            log: &self.log,
        }
        .run()
    }
}

fn stop_and_delete(rpc: &Rpc, session: &str, turn: Option<&str>) -> Result<(), String> {
    if let Some(turn) = turn {
        interrupt(rpc, session, turn);
    }
    rpc.request(
        "thread/delete",
        &json!({"threadId": session}),
        REQUEST_LIMIT,
    )
    .map(|_| ())
}

impl ForkHost for CodexForks {
    fn watch(&self, pane: &PaneId, report: StatusReport) -> PaneWatch {
        PaneWatch::host(&self.host, pane, self.waits.resubscribe, report)
    }

    fn session(&self, agent: &Agent) -> Option<String> {
        self.hooks.as_ref()?.named_session(&agent.pane_id)
    }

    fn unhooked(&self, agent: &Agent) -> Option<String> {
        let hooks = self.hooks.as_ref()?;
        if matches!(
            self.daemon_of(agent),
            Err(NoDaemon::WithoutDaemon | NoDaemon::Remote(_))
        ) {
            return None;
        }
        hooks.named_session(&agent.pane_id)?;
        hooks.hooked(&*self.host, agent).is_none().then(|| {
            "the Codex agent's session runs without the reviewer's Codex plugin: install it \
             with `make install`, then restart the agent"
                .into()
        })
    }

    fn can_move(&self, agent: &Agent) -> bool {
        self.hooks
            .as_ref()
            .is_some_and(|hooks| hooks.hooked(&*self.host, agent).is_some())
            && self.daemon_of(agent).is_ok()
    }

    fn unforkable(&self, agent: &Agent) -> Option<String> {
        if self.hooks.is_none() {
            return Some("the reviewer hears no agent's hooks: its log says why".into());
        }
        let daemon = match self.daemon_of(agent) {
            Ok(daemon) => daemon,
            Err(error @ (NoDaemon::WithoutDaemon | NoDaemon::Remote(_))) => {
                return Some(error.to_string());
            }
            Err(_) => return None,
        };
        match Self::guarded(&daemon) {
            Ok(true) => None,
            Err(error) => Some(error),
            Ok(false) => Some(
                "the reviewer's Codex plugin has no guard for run-ahead's forks: run `make \
                 install`"
                    .into(),
            ),
        }
    }

    fn point(&self, agent: &Agent) -> Result<ForkPoint, String> {
        let session = self
            .session(agent)
            .ok_or("the Codex agent named no session in a call to the reviewer's tools")?;
        let daemon = self.daemon_of(agent).map_err(|error| error.to_string())?;
        let rpc = connect(&daemon)?;
        let entry = Self::last_turn(&rpc, &session)?;
        Ok(ForkPoint {
            session,
            entry,
            model: None,
            host: fields::point(&daemon),
        })
    }

    fn last_entry(&self, point: &ForkPoint) -> Option<String> {
        let rpc = connect(&fields::daemon(&point.host)?).ok()?;
        Self::last_turn(&rpc, &point.session).ok().flatten()
    }

    fn create(&self, point: &ForkPoint) -> Result<OwnedFork, String> {
        let daemon = fields::daemon(&point.host).ok_or("the point names no Codex daemon")?;
        let hooks = self
            .hooks
            .as_ref()
            .ok_or("the reviewer hears no agent's hooks")?;
        let turn = Arc::new(TurnWatch::default());
        let rpc = daemon.connect(turn.handler(self.log.clone()))?;
        // Codex's `thread/fork` without overrides keeps the session's settings, and its answer
        // gives them.
        let forked = rpc.request(
            "thread/fork",
            &json!({"threadId": point.session, "excludeTurns": true}),
            REQUEST_LIMIT,
        )?;
        let fork = forked
            .pointer("/thread/id")
            .and_then(Value::as_str)
            .ok_or("the Codex daemon named no fork")?
            .to_owned();
        let agent_settings = json!({
            "approvalPolicy": forked.get("approvalPolicy"),
            "sandboxPolicy": forked.get("sandbox"),
            "approvalsReviewer": forked.get("approvalsReviewer"),
        });
        // Codex appends a settings change after the history at the next turn, which keeps the
        // agent's prompt cache.
        let guarded = hooks
            .root()
            .mark_codex_fork(&fork)
            .map_err(|error| format!("the fork's guard could not be armed: {error}"))
            .and_then(|()| {
                update_settings(
                    &rpc,
                    &fork,
                    &json!({
                        "approvalPolicy": "on-request",
                        "sandboxPolicy": {"type": "readOnly"},
                        "approvalsReviewer": "user",
                    }),
                )
            });
        if let Err(error) = guarded {
            let _ = stop_and_delete(&rpc, &fork, None);
            let _ = hooks.root().unmark_codex_fork(&fork);
            return Err(error);
        }
        turn.forked(&fork);
        self.live().insert(fork.clone(), LiveFork { rpc, turn });
        Ok(OwnedFork {
            session: fork,
            host: fields::fork(&daemon, &agent_settings),
        })
    }

    fn start(
        &self,
        start: ForkStart<'_>,
        prompt: String,
        ended: Box<dyn FnOnce(ForkEnd) + Send>,
    ) -> Result<HostFields, String> {
        let live = self.live();
        let fork = live
            .get(start.fork.session)
            .ok_or("the fork's connection to the Codex daemon is gone")?;
        fork.turn.starting(ended);
        let started = fork.rpc.request(
            "turn/start",
            &json!({"threadId": start.fork.session, "input": [{"type": "text", "text": prompt}]}),
            REQUEST_LIMIT,
        )?;
        let turn = started
            .pointer("/turn/id")
            .and_then(Value::as_str)
            .ok_or("the Codex daemon named no turn")?;
        fork.turn.started(turn);
        Ok(fields::started(turn))
    }

    fn discard(&self, fork: ForkTrace<'_>, done: Box<dyn FnOnce() + Send>) {
        let live = self.live().remove(fork.session);
        let session = fork.session.to_owned();
        let recorded = (fields::daemon(fork.host), fields::turn(fork.host));
        let (hooks, log) = (self.hooks.clone(), self.log.clone());
        self.in_background(move || {
            let deleted = match live {
                Some(live) => stop_and_delete(&live.rpc, &session, live.turn.id().as_deref()),
                None => match recorded {
                    (Some(daemon), turn) => connect(&daemon)
                        .and_then(|rpc| stop_and_delete(&rpc, &session, turn.as_deref())),
                    (None, _) => Err("its record names no Codex daemon".into()),
                },
            };
            if let Err(error) = deleted {
                log.write(&format!(
                    "the Codex fork {session} was not deleted: {error}"
                ));
            }
            if let Some(hooks) = hooks {
                let _ = hooks.root().unmark_codex_fork(&session);
            }
            done();
        });
    }

    fn switch(
        &self,
        switch: SwitchTo<'_>,
        done: Box<dyn FnOnce(Result<Agent, SwitchFailure>) + Send>,
    ) {
        let live = self.live().remove(switch.fork.session);
        let (fork, pane) = (OwnedFork::of(switch.fork), switch.pane.clone());
        let mover = self.mover();
        self.in_background(move || {
            let daemon = fields::daemon(&fork.host).ok_or_else(no_daemon);
            done(daemon.and_then(|daemon| {
                if !end_turn(live, &daemon, &fork, mover.waits) {
                    return Err(SwitchFailure::untyped(
                        "the fork's turn did not end once interrupted",
                    ));
                }
                mover.hand_over(&daemon, fork.trace());
                mover.resume(&pane, daemon, &fork.session, None, mover.waits.resume)
            }));
        });
    }

    fn resume(
        &self,
        pane: &PaneId,
        from: ForkTrace<'_>,
        session: &str,
        done: Box<dyn FnOnce(Result<Agent, SwitchFailure>) + Send>,
    ) {
        let (pane, session, fork) = (pane.clone(), session.to_owned(), OwnedFork::of(from));
        let mover = self.mover();
        self.in_background(move || {
            let daemon = fields::daemon(&fork.host).ok_or_else(no_daemon);
            done(daemon.and_then(|daemon| {
                let maybe_on = Some(fork.session.as_str());
                mover.resume(&pane, daemon, &session, maybe_on, mover.waits.settle)
            }));
        });
    }

    fn log(&self, line: &str) {
        self.log.write(line);
    }
}

#[cfg(test)]
#[path = "host.tests.rs"]
mod tests;
