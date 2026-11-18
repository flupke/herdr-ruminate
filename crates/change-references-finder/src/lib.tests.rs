use std::collections::BTreeMap;
use std::sync::Mutex;

use reviewer_config::ReviewerConfig;

use change_references::{ChangeReferences, PullRequest, Ticket};

use super::*;

/// jj and gh as a test sets them: a repository's remotes, its pull requests by head branch and
/// its issues' titles. It records the pull requests and issues asked for.
#[derive(Default)]
struct FakeTools {
    remotes: Option<String>,
    pull_requests: BTreeMap<String, FoundPullRequest>,
    issues: BTreeMap<u64, String>,
    asked: Mutex<Vec<String>>,
}

impl Tools for FakeTools {
    fn remotes(&self, _root: &Path) -> Option<String> {
        self.remotes.clone()
    }

    fn pull_request(
        &self,
        repository: &str,
        branch: &str,
        fork_owner: Option<&str>,
    ) -> Option<FoundPullRequest> {
        let fork = fork_owner
            .map(|owner| format!(" from {owner}"))
            .unwrap_or_default();
        self.asked
            .lock()
            .unwrap()
            .push(format!("pr {repository} {branch}{fork}"));
        self.pull_requests.get(branch).cloned()
    }

    fn issue_title(&self, repository: &str, number: u64) -> Option<String> {
        self.asked
            .lock()
            .unwrap()
            .push(format!("issue {repository} {number}"));
        self.issues.get(&number).cloned()
    }
}

fn on_github() -> FakeTools {
    FakeTools {
        remotes: Some("origin git@github.com:acme/shop.git\n".to_owned()),
        ..FakeTools::default()
    }
}

fn linear() -> LinearSettings {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("config.toml"),
        "[linear]\nworkspace = \"rayon\"\nteams = [\"FND\"]\n",
    )
    .unwrap();
    LinearSettings::from_config(&ReviewerConfig::from_dir(directory.path()).unwrap())
        .unwrap()
        .unwrap()
}

fn change(description: &str, names: &[&str]) -> SnapshotIdentity {
    SnapshotIdentity::Jj {
        change_id: "zzzz".to_owned().into(),
        snapshot_id: "abcd".to_owned().into(),
        description: description.to_owned(),
        display_id: "zz".to_owned(),
        names: names.iter().map(|name| (*name).to_owned()).collect(),
    }
}

fn find(
    tools: FakeTools,
    linear: Option<LinearSettings>,
    identity: &SnapshotIdentity,
) -> (ChangeReferences, Vec<String>) {
    let tools = Arc::new(tools);
    let finder = ReferencesFinder::with_tools(linear, tools.clone());
    let references = finder.find(Path::new("/repo"), identity);
    let asked = tools.asked.lock().unwrap().clone();
    (references, asked)
}

fn pull_request(number: u64, title: &str, body: &str) -> FoundPullRequest {
    FoundPullRequest {
        pull_request: PullRequest {
            number,
            title: title.to_owned(),
            url: format!("https://github.com/acme/shop/pull/{number}"),
        },
        body: body.to_owned(),
    }
}

fn ids(references: &ChangeReferences) -> Vec<&str> {
    references
        .tickets
        .iter()
        .map(|ticket| ticket.id.as_str())
        .collect()
}

#[test]
fn names_a_change_by_the_pull_request_of_one_of_its_bookmarks() {
    let mut tools = on_github();
    tools.pull_requests.insert(
        "luper/fix-login".to_owned(),
        pull_request(
            12,
            "Fix the login page",
            "Linear: https://linear.app/rayon/issue/FND-1871/login",
        ),
    );

    let (references, asked) = find(
        tools,
        None,
        &change("Fix login\n\nFixes #7", &["main-ish", "luper/fix-login"]),
    );

    assert_eq!(
        references.pull_request.as_ref().map(|found| found.number),
        Some(12)
    );
    assert_eq!(references.title(), Some("Fix the login page"));
    // The description's ticket first, then the pull request's.
    assert_eq!(ids(&references), ["#7", "FND-1871"]);
    // With a pull request, no issue's title is read.
    assert_eq!(
        asked,
        ["pr acme/shop main-ish", "pr acme/shop luper/fix-login"]
    );
}

#[test]
fn without_a_pull_request_names_a_change_by_its_github_issue() {
    let mut tools = on_github();
    tools.issues.insert(7, "Name past rounds".to_owned());

    let (references, asked) = find(tools, None, &change("WIP", &["t7-name-past-rounds"]));

    assert_eq!(references.pull_request, None);
    assert_eq!(
        references.tickets,
        [Ticket {
            id: "#7".to_owned(),
            url: "https://github.com/acme/shop/issues/7".to_owned(),
            title: Some("Name past rounds".to_owned()),
        }]
    );
    assert_eq!(references.title(), Some("Name past rounds"));
    assert_eq!(asked.last().unwrap(), "issue acme/shop 7");
}

#[test]
fn links_the_linear_issues_of_the_configured_teams_once() {
    let (references, _) = find(
        on_github(),
        Some(linear()),
        &change("Sync outage (FND-1871)", &["fnd-1871-sync-outage", "eng-4"]),
    );

    assert_eq!(ids(&references), ["FND-1871"]);
    assert_eq!(
        references.tickets[0].url,
        "https://linear.app/rayon/issue/FND-1871"
    );
    // A Linear issue has no title: the change keeps the description's.
    assert_eq!(references.title(), None);
}

#[test]
fn without_gh_keeps_the_tickets_the_texts_name() {
    // gh missing, logged out or offline: it finds neither pull request nor title.
    let (references, _) = find(on_github(), None, &change("Fixes #3", &["t3-slug"]));

    assert_eq!(references.pull_request, None);
    assert_eq!(ids(&references), ["#3"]);
    assert_eq!(references.title(), None);
}

#[test]
fn a_git_working_tree_has_no_references() {
    let identity = SnapshotIdentity::Git {
        base_tree: "tree".to_owned().into(),
        display_id: "abc".to_owned(),
        snapshot_id: "snap".to_owned().into(),
    };

    let (references, asked) = find(on_github(), Some(linear()), &identity);

    assert_eq!(references, ChangeReferences::default());
    assert!(asked.is_empty());
}

#[test]
fn the_ticket_of_the_bookmark_comes_first_and_names_the_change() {
    let mut tools = on_github();
    tools.issues.insert(7, "Name past rounds".to_owned());
    tools.issues.insert(110, "List past rounds".to_owned());

    let (references, _) = find(
        tools,
        None,
        &change(
            "Rename src/t2-utils\n\nFixes #110",
            &["luper/t7-name-past-rounds"],
        ),
    );

    // A `t2-` path in the description is no bookmark.
    assert_eq!(ids(&references), ["#7", "#110"]);
    assert_eq!(references.title(), Some("Name past rounds"));
}

#[test]
fn without_a_github_remote_keeps_the_references_that_name_their_repository() {
    let (references, asked) = find(
        FakeTools::default(),
        None,
        &change(
            "Fixes #3, Fixes other/tools#4, see https://github.com/acme/shop/issues/5",
            &[],
        ),
    );

    assert_eq!(ids(&references), ["other/tools#4", "acme/shop#5"]);
    // The title of the issue the change is for, from the repository it names.
    assert_eq!(asked, ["issue other/tools 4"]);
}

#[test]
fn an_issue_the_change_only_links_to_never_names_it() {
    let mut tools = on_github();
    tools.issues.insert(999, "Wakeups get lost".to_owned());

    let (references, asked) = find(
        tools,
        None,
        &change(
            "Work around lost wakeups\n\nBackground: https://github.com/acme/shop/issues/999",
            &[],
        ),
    );

    assert_eq!(ids(&references), ["#999"]);
    assert_eq!(references.title(), None);
    assert!(asked.iter().all(|asked| !asked.starts_with("issue")));
}

#[test]
fn in_a_forks_clone_asks_the_upstream_repository_for_the_forks_pull_request() {
    let tools = FakeTools {
        remotes: Some(
            "origin git@github.com:me/shop.git\nupstream https://github.com/acme/shop.git\n"
                .to_owned(),
        ),
        ..FakeTools::default()
    };

    let (references, asked) = find(tools, None, &change("Fixes #3", &["fix-login"]));

    assert_eq!(asked[0], "pr acme/shop fix-login from me");
    assert_eq!(
        references.tickets[0].url,
        "https://github.com/acme/shop/issues/3"
    );
}
