//! The commands that read what the texts of a change cannot say: jj's remotes, and the pull
//! requests and issues that gh reads on GitHub. Each gives up after [`TIMEOUT`], and any failure
//! reads as nothing found.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use change_references::PullRequest;
use serde::Deserialize;

use crate::FoundPullRequest;

/// How long a command may run before it is killed: gh waits on the network.
const TIMEOUT: Duration = Duration::from_secs(20);

/// How many pull requests of a branch's name gh lists, among which the finder skips those of
/// forks.
const LISTED: &str = "10";

/// What the finder asks of jj and gh. `None` when they cannot say.
pub trait Tools: Send + Sync {
    /// The remotes of the jj repository at `root`, as `jj git remote list` prints them.
    fn remotes(&self, root: &Path) -> Option<String>;
    /// The newest pull request of the GitHub repository `repository` (`owner/name`) whose head
    /// branch is `branch`, open or not, of that repository or of the fork of `fork_owner`, which
    /// the change's branches are pushed to: another fork's branch of the same name is another
    /// change.
    fn pull_request(
        &self,
        repository: &str,
        branch: &str,
        fork_owner: Option<&str>,
    ) -> Option<FoundPullRequest>;
    /// The title of the issue `number` of `repository`.
    fn issue_title(&self, repository: &str, number: u64) -> Option<String>;
}

/// The `jj` and `gh` on the `PATH`.
pub(crate) struct SystemTools;

impl Tools for SystemTools {
    fn remotes(&self, root: &Path) -> Option<String> {
        let mut command = Command::new("jj");
        command
            .args([
                "--ignore-working-copy",
                "--no-pager",
                "--color",
                "never",
                "-R",
            ])
            .arg(root)
            .args(["git", "remote", "list"]);
        run(command, TIMEOUT)
    }

    fn pull_request(
        &self,
        repository: &str,
        branch: &str,
        fork_owner: Option<&str>,
    ) -> Option<FoundPullRequest> {
        let output = run(
            gh([
                "pr",
                "list",
                "--repo",
                repository,
                "--head",
                branch,
                "--state",
                "all",
                "--limit",
                LISTED,
                "--json",
                "number,title,url,body,isCrossRepository,headRepositoryOwner",
            ]),
            TIMEOUT,
        )?;
        pull_request_of(&output, fork_owner)
    }

    fn issue_title(&self, repository: &str, number: u64) -> Option<String> {
        #[derive(Deserialize)]
        struct Issue {
            title: String,
        }
        let number = number.to_string();
        let output = run(
            gh([
                "issue", "view", &number, "--repo", repository, "--json", "title",
            ]),
            TIMEOUT,
        )?;
        serde_json::from_str::<Issue>(&output)
            .ok()
            .map(|issue| issue.title)
    }
}

/// The newest pull request from a branch of the repository itself, or of the fork of
/// `fork_owner`, in the list that `gh pr list --json
/// number,title,url,body,isCrossRepository,headRepositoryOwner` printed, newest first.
fn pull_request_of(output: &str, fork_owner: Option<&str>) -> Option<FoundPullRequest> {
    #[derive(Deserialize)]
    struct Owner {
        login: String,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Listed {
        number: u64,
        title: String,
        url: String,
        #[serde(default)]
        body: String,
        is_cross_repository: bool,
        head_repository_owner: Option<Owner>,
    }
    let listed: Vec<Listed> = serde_json::from_str(output).ok()?;
    let listed = listed.into_iter().find(|listed| {
        !listed.is_cross_repository
            || fork_owner.is_some_and(|fork_owner| {
                listed
                    .head_repository_owner
                    .as_ref()
                    .is_some_and(|owner| owner.login.eq_ignore_ascii_case(fork_owner))
            })
    })?;
    Some(FoundPullRequest {
        pull_request: PullRequest {
            number: listed.number,
            title: listed.title,
            url: listed.url,
        },
        body: listed.body,
    })
}

/// gh with `args`, which never prompts.
fn gh<'a>(args: impl IntoIterator<Item = &'a str>) -> Command {
    let mut command = Command::new("gh");
    command
        .args(args)
        .env("GH_PROMPT_DISABLED", "1")
        .env("NO_COLOR", "1");
    command
}

/// What `command` printed, once it succeeded; `None` when it cannot start, fails, or prints
/// for longer than `timeout`, which kills it.
fn run(mut command: Command, timeout: Duration) -> Option<String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let (sent, output) = mpsc::channel();
    // Read as it runs: a full pipe would keep it from ending. A killed command's children may
    // hold the pipe open: the reader is then left behind.
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = sent.send(stdout.read_to_string(&mut text).ok().map(|_| text));
    });
    let Ok(Some(text)) = output.recv_timeout(timeout) else {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    };
    // Its output closed: it ended, or is about to.
    child
        .wait()
        .ok()
        .filter(std::process::ExitStatus::success)?;
    Some(text)
}

#[cfg(test)]
#[path = "tools.tests.rs"]
mod tests;
