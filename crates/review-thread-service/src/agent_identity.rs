use herdr_client::protocol::{Agent, AgentPort, AgentSession, PaneId, TabId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SessionComparison {
    Exact,
    IgnoringSource,
}

impl SessionComparison {
    fn same(self, bound: &AgentSession, current: &AgentSession) -> bool {
        match self {
            Self::Exact => bound == current,
            Self::IgnoringSource => {
                bound.agent == current.agent
                    && bound.kind == current.kind
                    && bound.value == current.value
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProcessBinding {
    KeptAfterSession,
    ReleasedBySession,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct IdentityRules {
    session: SessionComparison,
    process: ProcessBinding,
}

impl IdentityRules {
    pub(crate) const REVIEW_ACCESS: Self = Self {
        session: SessionComparison::Exact,
        process: ProcessBinding::KeptAfterSession,
    };
    pub(crate) const PINNED_AGENT: Self = Self {
        session: SessionComparison::IgnoringSource,
        process: ProcessBinding::ReleasedBySession,
    };
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum SessionBinding {
    Following(Option<AgentSession>),
    Unbound,
    Bound(AgentSession),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Change {
    Agent,
    Tab,
    ProcessGroup,
    Session,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Verdict {
    Same,
    SessionMissing,
    Changed(Change),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum IdentityError {
    Port(String),
    ProcessUnidentified,
}

impl IdentityError {
    pub(crate) fn into_message(self, process_unidentified: &str) -> String {
        match self {
            Self::Port(error) => error,
            Self::ProcessUnidentified => process_unidentified.to_owned(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct AgentIdentity {
    pane_id: PaneId,
    tab_id: TabId,
    agent: Option<String>,
    session: SessionBinding,
    process_group: Option<u32>,
    rules: IdentityRules,
}

impl AgentIdentity {
    pub(crate) fn new(agent: &Agent, rules: IdentityRules) -> Self {
        Self {
            pane_id: agent.pane_id.clone(),
            tab_id: agent.tab_id.clone(),
            agent: agent.agent.clone(),
            session: agent
                .agent_session
                .clone()
                .map_or(SessionBinding::Unbound, SessionBinding::Bound),
            process_group: None,
            rules,
        }
    }

    pub(crate) fn following(mut self) -> Self {
        self.session = SessionBinding::Following(match self.session {
            SessionBinding::Bound(session) => Some(session),
            SessionBinding::Following(session) => session,
            SessionBinding::Unbound => None,
        });
        self
    }

    pub(crate) fn bind_process_without_session(
        mut self,
        port: &dyn AgentPort,
    ) -> Result<Self, IdentityError> {
        if matches!(
            self.session,
            SessionBinding::Unbound | SessionBinding::Following(None)
        ) {
            self.process_group = Some(process_group(port, &self.pane_id)?);
        }
        Ok(self)
    }

    pub(crate) fn seal(&mut self) {
        if let SessionBinding::Following(session) = &mut self.session {
            self.session = session
                .take()
                .map_or(SessionBinding::Unbound, SessionBinding::Bound);
        }
    }

    pub(crate) fn check(
        &mut self,
        port: &dyn AgentPort,
        current: &Agent,
    ) -> Result<Verdict, IdentityError> {
        if current.pane_id != self.pane_id || current.agent != self.agent {
            return Ok(Verdict::Changed(Change::Agent));
        }
        if current.tab_id != self.tab_id {
            return Ok(Verdict::Changed(Change::Tab));
        }
        if let Some(group) = self.process_group
            && process_group(port, &current.pane_id)? != group
        {
            return Ok(Verdict::Changed(Change::ProcessGroup));
        }
        let verdict = self.compare_session(current.agent_session.as_ref());
        if verdict == Verdict::Same {
            self.adopt(current.agent_session.as_ref());
        }
        Ok(verdict)
    }

    fn compare_session(&self, current: Option<&AgentSession>) -> Verdict {
        match (&self.session, current) {
            (SessionBinding::Following(_) | SessionBinding::Unbound, _) => Verdict::Same,
            (SessionBinding::Bound(_), None) => Verdict::SessionMissing,
            (SessionBinding::Bound(bound), Some(current)) => {
                if self.rules.session.same(bound, current) {
                    Verdict::Same
                } else {
                    Verdict::Changed(Change::Session)
                }
            }
        }
    }

    fn adopt(&mut self, current: Option<&AgentSession>) {
        if let SessionBinding::Following(latest) = &mut self.session {
            *latest = current.cloned();
        }
        let Some(current) = current else {
            return;
        };
        if self.session == SessionBinding::Unbound {
            self.session = SessionBinding::Bound(current.clone());
        }
        if self.rules.process == ProcessBinding::ReleasedBySession {
            self.process_group = None;
        }
    }
}

fn process_group(port: &dyn AgentPort, pane: &PaneId) -> Result<u32, IdentityError> {
    port.pane_process_info(pane)
        .map_err(|error| IdentityError::Port(error.to_string()))?
        .foreground_process_group_id
        .filter(|group| *group != 0)
        .ok_or(IdentityError::ProcessUnidentified)
}

#[cfg(test)]
#[path = "agent_identity.tests.rs"]
mod tests;
