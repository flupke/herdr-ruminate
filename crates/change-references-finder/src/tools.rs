use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use change_references::PullRequest;
use serde::Deserialize;

use crate::FoundPullRequest;

// gh waits on the network.
const TIMEOUT: Duration = Duration::from_secs(20);

const LISTED: &str = "10";

pub trait Tools: Send + Sync {
    fn remotes(&self, root: &Path) -> Option<String>;
    fn pull_request(
        &self,
        repository: &str,
        branch: &str,
        fork_owner: Option<&str>,
    ) -> Option<FoundPullRequest>;
    fn issue_title(&self, repository: &str, number: u64) -> Option<String>;
}

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

fn gh<'a>(args: impl IntoIterator<Item = &'a str>) -> Command {
    let mut command = Command::new("gh");
    command
        .args(args)
        .env("GH_PROMPT_DISABLED", "1")
        .env("NO_COLOR", "1");
    command
}

fn run(mut command: Command, timeout: Duration) -> Option<String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let (sent, output) = mpsc::channel();
    // A full pipe keeps the command from ending; a killed command's children may hold the
    // pipe open.
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = sent.send(stdout.read_to_string(&mut text).ok().map(|_| text));
    });
    let Ok(Some(text)) = output.recv_timeout(timeout) else {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    };
    child
        .wait()
        .ok()
        .filter(std::process::ExitStatus::success)?;
    Some(text)
}

#[cfg(test)]
#[path = "tools.tests.rs"]
mod tests;
