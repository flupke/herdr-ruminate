//! What the fake agent and its forks reported on the session's event socket, as the agent
//! driving the session reads it: each fork run-ahead started, with its prompt and how far it got,
//! and the sessions the fake agent ran.

use fake_agent::{Command, Event, Reported, Role};
use serde::Serialize;

use super::session::AGENT;

/// One fork, in the order the forks started.
#[derive(Debug, Eq, PartialEq, Serialize)]
pub(crate) struct Fork {
    /// Its session, which the agent driving the session names to play it.
    pub(crate) session: String,
    /// The session it copies.
    parent: String,
    /// The choice it takes the turn of, from its prompt.
    choice: Option<String>,
    /// The prompt it read, the turn's files appended.
    pub(crate) prompt: String,
    /// Its output shows a tool call and its result: its submit.
    submitted: bool,
    /// It ended its turn without a submit.
    finished: bool,
    /// Its process ended, however it ended: the reviewer stops a fork it discards.
    pub(crate) exited: bool,
}

/// The sessions of the agent in the pane.
#[derive(Debug, Default, Eq, PartialEq, Serialize)]
pub(crate) struct AgentSessions {
    /// The session it started last, once its `SessionStart` hooks, the reviewer's among them,
    /// ran.
    pub(crate) session: Option<String>,
    /// The sessions it resumed with Claude Code's `/resume`, in order.
    resumed: Vec<String>,
}

/// The forks among the events `received`, in the order they started.
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

/// The sessions of the agent among the events `received`.
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

/// The rest of the first line of `prompt` that starts with `label`.
pub(crate) fn prompt_line(prompt: &str, label: &str) -> Option<String> {
    prompt
        .lines()
        .find_map(|line| line.strip_prefix(label))
        .map(str::to_owned)
}

#[cfg(test)]
#[path = "forks.tests.rs"]
mod tests;
