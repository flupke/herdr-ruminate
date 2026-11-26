use fake_agent::{Command, Event, Reported, Role};
use serde::Serialize;

use super::session::AGENT;

#[derive(Debug, Eq, PartialEq, Serialize)]
pub(crate) struct Fork {
    pub(crate) session: String,
    parent: String,
    choice: Option<String>,
    pub(crate) prompt: String,
    submitted: bool,
    finished: bool,
    pub(crate) exited: bool,
}

#[derive(Debug, Default, Eq, PartialEq, Serialize)]
pub(crate) struct AgentSessions {
    pub(crate) session: Option<String>,
    resumed: Vec<String>,
}

pub(crate) fn forks(received: &[Reported]) -> Vec<Fork> {
    let mut forks: Vec<Fork> = Vec::new();
    for reported in received {
        let Role::Fork { session } = &reported.from else {
            continue;
        };
        if let Event::ForkStarted { parent, prompt } = &reported.event {
            forks.push(Fork {
                session: session.clone(),
                parent: parent.clone(),
                choice: prompt_line(prompt, "Selected option ID: "),
                prompt: prompt.clone(),
                submitted: false,
                finished: false,
                exited: false,
            });
            continue;
        }
        let Some(fork) = forks.iter_mut().find(|fork| fork.session == *session) else {
            continue;
        };
        match &reported.event {
            Event::Obeyed {
                command: Command::CallTool { .. },
            } => fork.submitted = true,
            Event::TurnEnded { .. } => fork.finished = !fork.submitted,
            Event::Disconnected => fork.exited = true,
            _ => {}
        }
    }
    forks
}

pub(crate) fn agent_sessions(received: &[Reported]) -> AgentSessions {
    let mut sessions = AgentSessions::default();
    let agent = Role::agent(AGENT);
    for reported in received.iter().filter(|reported| reported.from == agent) {
        match &reported.event {
            Event::SessionStarted { session, .. } => {
                sessions.session = Some(session.clone());
            }
            Event::ResumeReceived { session } => sessions.resumed.push(session.clone()),
            _ => {}
        }
    }
    sessions
}

pub(crate) fn prompt_line(prompt: &str, label: &str) -> Option<String> {
    prompt
        .lines()
        .find_map(|line| line.strip_prefix(label))
        .map(str::to_owned)
}

#[cfg(test)]
#[path = "forks.tests.rs"]
mod tests;
