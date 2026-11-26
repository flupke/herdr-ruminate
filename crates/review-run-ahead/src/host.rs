use std::sync::Arc;
use std::thread;
use std::time::Duration;

use herdr_client::client::EventCanceller;
use herdr_client::protocol::{Agent, AgentHost, AgentStatus, PaneId};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::record::TokenUsage;

pub type StatusReport = Box<dyn Fn(AgentStatus) + Send + Sync>;

pub struct PaneWatch {
    _stop: Box<dyn Send>,
}

impl PaneWatch {
    pub fn new(stop: impl Send + 'static) -> Self {
        Self {
            _stop: Box::new(stop),
        }
    }

    pub fn host(
        host: &Arc<dyn AgentHost>,
        pane: &PaneId,
        resubscribe: Duration,
        report: StatusReport,
    ) -> Self {
        let canceller = EventCanceller::default();
        let (host, pane, stop) = (Arc::clone(host), pane.clone(), canceller.clone());
        thread::spawn(move || {
            let mut dropped = false;
            while !stop.is_cancelled() {
                if std::mem::take(&mut dropped) {
                    report(AgentStatus::Working);
                }
                let watched = host.forward_agent_status(&pane, &stop, &mut |status| {
                    report(status);
                    true
                });
                match watched {
                    Ok(()) | Err(herdr_client::Error::Cancelled) => return,
                    Err(_) => {
                        dropped = true;
                        thread::sleep(resubscribe);
                    }
                }
            }
        });
        Self::new(CancelOnDrop(canceller))
    }
}

struct CancelOnDrop(EventCanceller);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

impl std::fmt::Debug for PaneWatch {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("PaneWatch")
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(transparent)]
pub struct HostFields(Map<String, Value>);

impl HostFields {
    pub fn get<T: DeserializeOwned>(&self, name: &str) -> Option<T> {
        T::deserialize(self.0.get(name)?).ok()
    }

    pub fn has(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }

    pub fn set(&mut self, name: &str, value: impl Serialize) {
        if let Ok(value) = serde_json::to_value(value) {
            self.0.insert(name.to_owned(), value);
        }
    }

    #[must_use]
    pub fn with(mut self, more: Self) -> Self {
        self.0.extend(more.0);
        self
    }
}

#[derive(Clone, Debug)]
pub struct ForkPoint {
    pub session: String,
    pub entry: Option<String>,
    pub model: Option<String>,
    pub host: HostFields,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnedFork {
    pub session: String,
    pub host: HostFields,
}

impl OwnedFork {
    pub fn of(trace: ForkTrace<'_>) -> Self {
        Self {
            session: trace.session.to_owned(),
            host: trace.host.clone(),
        }
    }

    pub fn trace(&self) -> ForkTrace<'_> {
        ForkTrace {
            session: &self.session,
            host: &self.host,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ForkStart<'a> {
    pub point: &'a ForkPoint,
    pub fork: ForkTrace<'a>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ForkEnd {
    pub exit: String,
    pub usage: TokenUsage,
    pub finished: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct ForkTrace<'a> {
    pub session: &'a str,
    pub host: &'a HostFields,
}

#[derive(Clone, Copy, Debug)]
pub struct SwitchTo<'a> {
    pub pane: &'a PaneId,
    pub fork: ForkTrace<'a>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SwitchFailure {
    pub error: String,
    pub typed: bool,
}

impl SwitchFailure {
    pub fn untyped(error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            typed: false,
        }
    }

    fn joined() -> Self {
        Self::untyped(
            "text in the agent's input box joined the /resume, so the reviewer stopped both; \
             clear the agent's input box, then Retry",
        )
    }

    pub fn blocked(prompt: &str, command: &str) -> Self {
        if prompt.contains(command) {
            Self::joined()
        } else {
            Self::prompt_meanwhile()
        }
    }

    fn prompt_meanwhile() -> Self {
        Self {
            error: "a prompt reached the agent while it resumed the session, and the reviewer \
                    stopped it"
                .into(),
            typed: true,
        }
    }
}

pub fn resume_block(command: &str) -> String {
    format!(
        "The progressive reviewer stopped this prompt: it typed `{command}` while text waited \
         in the input box, and both would have run as one prompt. Clear the input box before \
         you Retry the answer in the reviewer."
    )
}

pub trait ForkHost: Send + Sync {
    fn watch(&self, pane: &PaneId, report: StatusReport) -> PaneWatch;

    fn session(&self, agent: &Agent) -> Option<String>;

    fn unhooked(&self, agent: &Agent) -> Option<String>;

    fn can_move(&self, agent: &Agent) -> bool;

    fn unforkable(&self, agent: &Agent) -> Option<String>;

    fn point(&self, agent: &Agent) -> Result<ForkPoint, String>;

    fn last_entry(&self, point: &ForkPoint) -> Option<String>;

    fn create(&self, point: &ForkPoint) -> Result<OwnedFork, String>;

    fn start(
        &self,
        start: ForkStart<'_>,
        prompt: String,
        ended: Box<dyn FnOnce(ForkEnd) + Send>,
    ) -> Result<HostFields, String>;

    fn discard(&self, fork: ForkTrace<'_>, done: Box<dyn FnOnce() + Send>);

    fn switch(
        &self,
        switch: SwitchTo<'_>,
        done: Box<dyn FnOnce(Result<Agent, SwitchFailure>) + Send>,
    );

    fn resume(
        &self,
        pane: &PaneId,
        from: ForkTrace<'_>,
        session: &str,
        done: Box<dyn FnOnce(Result<Agent, SwitchFailure>) + Send>,
    );

    fn log(&self, line: &str);
}
