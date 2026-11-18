//! The pull request and the tickets of a reviewed change, which name its past rounds on the
//! Explore page. The `change-references-finder` crate finds them once a round starts; the review
//! store saves them beside the round.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The pull request and the tickets of the change a round reviewed, as the reviewer found them
/// when the round started.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, TS)]
pub struct ChangeReferences {
    /// The pull request whose head branch is one of the change's names.
    pub pull_request: Option<PullRequest>,
    /// The tickets the change names: herdr-bowtie's ticket of its names first, then those of
    /// its names, its description and its pull request's body, in that order.
    pub tickets: Vec<Ticket>,
}

impl ChangeReferences {
    /// What names the change: the pull request's title, else the title of its first ticket that
    /// has one; `None` without either.
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

    /// The references of a change whose rounds saved `saved`, newest first: the pull request of
    /// the newest round that found one, and the tickets of the newest round that found any,
    /// each with the newest title a round read for it. A round started while gh was offline
    /// still finds the tickets its texts name, but no pull request and no title: an older
    /// round's then still name the change.
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

/// A GitHub pull request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    /// The address of its page.
    pub url: String,
}

/// The issue of a tracker that a change is for: a GitHub issue or a Linear issue.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
pub struct Ticket {
    /// Its number, as its tracker writes it: `#157`, `flupke/herdr#3` in another repository,
    /// `ENG-1871`.
    pub id: String,
    /// The address of its page.
    pub url: String,
    /// Its title, when the reviewer read it: a GitHub issue's, without a pull request.
    pub title: Option<String>,
}

#[cfg(test)]
#[path = "lib.tests.rs"]
mod tests;
