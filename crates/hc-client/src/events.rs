use std::collections::HashMap;

use herdr_client::protocol::{AgentStatus, HerdrEvent, PaneId, TabId, WorkspaceId};
use herdr_client::{Error, Result};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::terminal::{Foreground, SESSION_LABEL, Status, Terminal, place_of, running};

#[derive(Debug, Deserialize)]
pub(crate) struct Event {
    #[serde(rename = "type")]
    pub(crate) kind: String,
    #[serde(default)]
    terminal: Option<String>,
    #[serde(default)]
    data: Value,
}

#[derive(Debug, Deserialize)]
pub(crate) struct StatusChange {
    #[serde(default)]
    status: Option<Status>,
}

#[derive(Debug, Deserialize)]
struct LabelsChange {
    #[serde(default)]
    labels: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    record: Option<Terminal>,
}

impl Event {
    pub(crate) fn data<T: DeserializeOwned>(&self) -> Result<T> {
        T::deserialize(&self.data).map_err(|source| Error::Json {
            operation: "read a herdr-core event",
            source,
        })
    }

    pub(crate) fn saved_document(&self) -> Option<&str> {
        (self.kind == "doc").then(|| self.data.get("name")?.as_str())?
    }

    pub(crate) fn ends_program(&self) -> bool {
        matches!(self.kind.as_str(), "finished" | "removed" | "lost")
    }
}

#[derive(Debug, Default)]
pub(crate) struct Roster {
    terminals: HashMap<String, Known>,
}

#[derive(Clone, Debug, Default)]
struct Known {
    place: Option<(WorkspaceId, TabId)>,
    status: Option<Status>,
    foreground: Option<Foreground>,
    session: Option<String>,
}

type Held<'a> = (&'a (WorkspaceId, TabId), Status);

impl Known {
    fn of(terminal: Terminal) -> Self {
        Self {
            place: terminal.place(),
            session: terminal.labels.get(SESSION_LABEL).cloned(),
            status: terminal.status,
            foreground: terminal.foreground,
        }
    }

    fn agent(&self) -> Option<Held<'_>> {
        self.place.as_ref().zip(running(
            self.status.as_ref(),
            self.foreground.as_ref(),
            self.session.as_deref(),
        ))
    }
}

impl Roster {
    pub(crate) fn new(snapshot: Vec<Terminal>) -> Self {
        Self {
            terminals: snapshot
                .into_iter()
                .map(|terminal| (terminal.address.clone(), Known::of(terminal)))
                .collect(),
        }
    }

    pub(crate) fn agents(&self) -> impl Iterator<Item = PaneId> + '_ {
        self.terminals
            .iter()
            .filter(|(_, known)| known.agent().is_some())
            .map(|(terminal, _)| PaneId(terminal.clone()))
    }

    pub(crate) fn observe(&mut self, event: &Event) -> Result<Option<HerdrEvent>> {
        let Some(terminal) = &event.terminal else {
            return Ok(None);
        };
        let after = match event.kind.as_str() {
            // herdr-core sends a lost terminal back as `restarted`, with its whole record.
            "created" | "restarted" => Known::of(event.data()?),
            "status" => Known {
                status: event.data::<StatusChange>()?.status,
                ..self.known(terminal)
            },
            // herdr-core 0.1.18 sends `foreground` events, though `hc schema` does not list them.
            "foreground" => Known {
                foreground: event.data()?,
                ..self.known(terminal)
            },
            "labels" => {
                let change: LabelsChange = event.data()?;
                match change.record {
                    Some(record) if !self.terminals.contains_key(terminal) => Known::of(record),
                    _ => Known {
                        place: place_of(&change.labels),
                        session: change.labels.get(SESSION_LABEL).cloned(),
                        ..self.known(terminal)
                    },
                }
            }
            _ if event.ends_program() => Known::default(),
            _ => return Ok(None),
        };
        let before = self.terminals.insert(terminal.clone(), after.clone());
        if event.kind == "removed" {
            self.terminals.remove(terminal);
        }
        let arrival_or_departure = change(
            terminal,
            before.as_ref().and_then(Known::agent),
            after.agent(),
        );
        let session_changed =
            after.agent().is_some() && before.and_then(|before| before.session) != after.session;
        Ok(arrival_or_departure.or_else(|| {
            session_changed.then(|| HerdrEvent::AgentSessionChanged(PaneId(terminal.clone())))
        }))
    }

    fn known(&self, terminal: &str) -> Known {
        self.terminals.get(terminal).cloned().unwrap_or_default()
    }
}

fn change(terminal: &str, before: Option<Held<'_>>, after: Option<Held<'_>>) -> Option<HerdrEvent> {
    let pane_id = PaneId(terminal.to_owned());
    match (before, after) {
        (Some(((workspace, _), was)), None) => Some(HerdrEvent::AgentDetected {
            pane_id,
            workspace_id: workspace.clone(),
            agent: was.agent(),
            released: true,
            final_status: Some(was.agent_status()),
        }),
        (Some((was_at, was)), Some((at, now))) if was_at == at && was.agent() == now.agent() => {
            None
        }
        (_, Some(((workspace, _), now))) => Some(HerdrEvent::AgentDetected {
            pane_id,
            workspace_id: workspace.clone(),
            agent: now.agent(),
            released: false,
            final_status: None,
        }),
        (None, None) => None,
    }
}

pub(crate) fn status_of(change: &StatusChange) -> AgentStatus {
    change
        .status
        .as_ref()
        .map_or(AgentStatus::Unknown, Status::agent_status)
}

#[cfg(test)]
#[path = "events.tests.rs"]
mod tests;
