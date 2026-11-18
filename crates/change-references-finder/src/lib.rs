//! Finds the pull request and the tickets of a reviewed change, which name its past rounds on
//! the Explore page.
//!
//! [`ReferencesFinder::find`] looks for them once a round starts, on a thread of its own, since
//! it runs `jj` and `gh`, which can be slow, missing, logged out or offline: any of these leaves
//! out what it would have found, never more. It reads:
//!
//! - the GitHub repository of the jj repository's remote (`upstream`, then `origin`);
//! - the pull request of that repository whose head branch is one of the change's names (its
//!   local bookmarks, then its jj workspaces), in the repository or in the fork it pushes to,
//!   through `gh`;
//! - the tickets that the change's names (herdr-bowtie's `t157-slug`), its description and the
//!   pull request's body name: GitHub issues (`Fixes #157`, issue links) and Linear issues (the
//!   identifiers of the teams of the `[linear]` table of `config.toml`, and links to
//!   linear.app);
//! - without a pull request, through `gh`, the title of the first GitHub ticket that the change
//!   says it is for: its bookmark's, else one after a word such as "Fixes". An issue link alone
//!   may be one the change only mentions: it never names the change.
//!
//! The reviewer has no Linear API key: a Linear issue has no title.

mod text;
mod tools;

use std::path::Path;
use std::sync::Arc;

use change_references::{ChangeReferences, PullRequest, Ticket};
use linear_issues::LinearSettings;
use review_repository::repository::SnapshotIdentity;

use crate::text::GitHubRepository;
use crate::tools::SystemTools;
pub use crate::tools::Tools;

/// Finds the pull request and the tickets of a change, with the commands `tools` runs.
#[derive(Clone)]
pub struct ReferencesFinder {
    linear: Option<LinearSettings>,
    tools: Arc<dyn Tools>,
}

impl ReferencesFinder {
    /// A finder that runs jj and gh, and knows the Linear teams of `linear`.
    pub fn new(linear: Option<LinearSettings>) -> Self {
        Self::with_tools(linear, Arc::new(SystemTools))
    }

    /// A finder that runs its commands with `tools`.
    pub fn with_tools(linear: Option<LinearSettings>, tools: Arc<dyn Tools>) -> Self {
        Self { linear, tools }
    }

    /// The references of the change `identity`, in the jj repository at `root`. It blocks on
    /// jj and gh. A Git working tree has none.
    pub fn find(&self, root: &Path, identity: &SnapshotIdentity) -> ChangeReferences {
        let SnapshotIdentity::Jj { description, .. } = identity else {
            return ChangeReferences::default();
        };
        let names = identity.names();
        let remotes = self
            .tools
            .remotes(root)
            .as_deref()
            .and_then(text::github_remotes);
        let repository = remotes.as_ref().map(|remotes| &remotes.repository);
        let pull_request = remotes.as_ref().and_then(|remotes| {
            names.iter().find_map(|name| {
                self.tools.pull_request(
                    remotes.repository.slug(),
                    name,
                    remotes.fork_owner.as_deref(),
                )
            })
        });
        let body = pull_request
            .as_ref()
            .map(|found| found.body.as_str())
            .unwrap_or_default();
        let mut tickets = self.tickets(names, [description.as_str(), body], repository);
        if pull_request.is_none() {
            self.read_title(&mut tickets);
        }
        ChangeReferences {
            pull_request: pull_request.map(|found| found.pull_request),
            tickets: tickets.found,
        }
    }

    /// The tickets that the change's `names` and `texts` name, the change's repository being
    /// `repository`: herdr-bowtie's ticket of its names first, since herdr-bowtie names a
    /// workspace and its bookmark after the ticket it works on, then those of its names and its
    /// texts, in that order.
    fn tickets(
        &self,
        names: &[String],
        texts: [&str; 2],
        repository: Option<&GitHubRepository>,
    ) -> Tickets {
        let mut tickets = Tickets::default();
        if let Some(repository) = repository {
            for number in names.iter().filter_map(|name| text::bookmark_ticket(name)) {
                tickets.add(
                    text::github_ticket(Some(repository), repository, number),
                    true,
                );
            }
        }
        for text in names.iter().map(String::as_str).chain(texts) {
            for (ticket, said) in text::github_tickets(text, repository) {
                tickets.add(ticket, said);
            }
            let linear = self.linear.iter().flat_map(|linear| linear.issues([text]));
            for issue in linear {
                let ticket = Ticket {
                    id: issue.id,
                    url: issue.url,
                    title: None,
                };
                tickets.add(ticket, false);
            }
            for ticket in text::linear_links(text) {
                tickets.add(ticket, false);
            }
        }
        tickets
    }

    /// Reads the title of the GitHub ticket the change says it is for, the first one, with gh.
    fn read_title(&self, tickets: &mut Tickets) {
        let Some(named) = &tickets.named else {
            return;
        };
        if let Some(ticket) = tickets.found.iter_mut().find(|ticket| &ticket.url == named)
            && let Some((repository, number)) = text::github_issue(&ticket.url)
        {
            ticket.title = self.tools.issue_title(repository.slug(), number);
        }
    }
}

/// A pull request as gh lists it, with its body, where tickets may be named.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FoundPullRequest {
    pub pull_request: PullRequest,
    pub body: String,
}

/// Tickets, each once by its address, in the order they first came.
#[derive(Default)]
struct Tickets {
    found: Vec<Ticket>,
    /// The address of the first GitHub ticket the change says it is for, whose title names
    /// the change without a pull request. A Linear issue has no title to read.
    named: Option<String>,
}

impl Tickets {
    /// Adds `ticket`, which the change says it is for when `said`, unless it came before.
    fn add(&mut self, ticket: Ticket, said: bool) {
        if said && self.named.is_none() && text::github_issue(&ticket.url).is_some() {
            self.named = Some(ticket.url.clone());
        }
        if self.found.iter().all(|known| known.url != ticket.url) {
            self.found.push(ticket);
        }
    }
}

#[cfg(test)]
#[path = "lib.tests.rs"]
mod tests;
