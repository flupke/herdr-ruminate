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

#[derive(Clone)]
pub struct ReferencesFinder {
    linear: Option<LinearSettings>,
    tools: Arc<dyn Tools>,
}

impl ReferencesFinder {
    pub fn new(linear: Option<LinearSettings>) -> Self {
        Self::with_tools(linear, Arc::new(SystemTools))
    }

    pub fn with_tools(linear: Option<LinearSettings>, tools: Arc<dyn Tools>) -> Self {
        Self { linear, tools }
    }

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FoundPullRequest {
    pub pull_request: PullRequest,
    pub body: String,
}

#[derive(Default)]
struct Tickets {
    found: Vec<Ticket>,
    named: Option<String>,
}

impl Tickets {
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
