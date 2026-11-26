use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, TS)]
pub struct ChangeReferences {
    pub pull_request: Option<PullRequest>,
    pub tickets: Vec<Ticket>,
}

impl ChangeReferences {
    pub fn title(&self) -> Option<&str> {
        self.pull_request
            .as_ref()
            .map(|pull_request| pull_request.title.as_str())
            .or_else(|| {
                self.tickets
                    .iter()
                    .find_map(|ticket| ticket.title.as_deref())
            })
            .map(str::trim)
            .filter(|title| !title.is_empty())
    }

    pub fn of_rounds<'a>(saved: impl IntoIterator<Item = &'a Self>) -> Self {
        let saved: Vec<&Self> = saved.into_iter().collect();
        let mut merged = Self {
            pull_request: saved
                .iter()
                .find_map(|references| references.pull_request.clone()),
            tickets: saved
                .iter()
                .find(|references| !references.tickets.is_empty())
                .map(|references| references.tickets.clone())
                .unwrap_or_default(),
        };
        for ticket in &mut merged.tickets {
            if ticket.title.is_none() {
                ticket.title = saved
                    .iter()
                    .flat_map(|references| &references.tickets)
                    .filter(|read| read.url == ticket.url)
                    .find_map(|read| read.title.clone());
            }
        }
        merged
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
pub struct Ticket {
    pub id: String,
    pub url: String,
    pub title: Option<String>,
}

#[cfg(test)]
#[path = "lib.tests.rs"]
mod tests;
