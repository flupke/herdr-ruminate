#![allow(
    clippy::missing_panics_doc,
    reason = "test fixture methods fail fast by design"
)]

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use review_repository::repository::{PollResult, RepoType, Repository, Snapshot};

pub mod codex;
mod port;
pub use port::TestPort;
mod detection_rules;
mod herdr;
mod herdr_events;
pub mod turn;
pub use herdr::{PrivateHerdrServer, write_reviewer_settings};
pub use herdr_events::{AgentStatusWatch, HerdrEventWatch};

pub const GUARD: Duration = Duration::from_secs(30);

pub fn eventually(timeout: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if condition() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(10));
    }
}

pub fn refuses_connections(address: std::net::SocketAddr) -> bool {
    std::net::TcpStream::connect(address).is_err()
}

pub fn wait_with_output_within(child: Child, guard: Duration) -> Output {
    let pid = child.id().to_string();
    let (finished, done) = mpsc::channel::<()>();
    let killer = thread::spawn(move || {
        if done.recv_timeout(guard) == Err(mpsc::RecvTimeoutError::Timeout) {
            let _ = Command::new("kill").args(["-KILL", &pid]).status();
        }
    });
    let output = child.wait_with_output().unwrap();
    drop(finished);
    killer.join().unwrap();
    output
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JjLayout {
    NonColocated,
    Colocated,
}

fn fixture_directory(parent: &Path) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("review-repository-")
        .tempdir_in(parent)
        .unwrap()
}

pub fn repository_fixture(repository_type: RepoType) -> Box<dyn ReviewRepositoryFixture> {
    repository_fixture_in(repository_type, &std::env::temp_dir())
}

pub fn repository_fixture_in(
    repository_type: RepoType,
    parent: &Path,
) -> Box<dyn ReviewRepositoryFixture> {
    match repository_type {
        RepoType::Git => Box::new(GitFixture::new_in(parent)),
        RepoType::Jj => Box::new(JjFixture::new_in(JjLayout::NonColocated, parent)),
    }
}

pub fn complete_repository_snapshot(repository: &Repository) -> Snapshot {
    match repository.poll().unwrap() {
        PollResult::Complete(snapshot) => snapshot,
        PollResult::ChangedDuringPoll => panic!("fixture changed during a synchronous poll"),
    }
}

pub trait ReviewRepositoryFixture: Send {
    fn root(&self) -> &Path;
    fn write(&self, relative_path: &str, contents: &[u8]);
    fn remove(&self, relative_path: &str);
    fn rename(&self, from: &str, to: &str);
    fn symlink(&self, target: &str, link: &str);
    fn revision_id(&self) -> String;
    fn new_change(&self, description: &str);
    fn edit(&self, revision: &str);
    fn rewrite_base_without_reviewed_file_change(&self);
    fn add_workspace(&self, name: &str) -> PathBuf;
}

#[derive(Debug)]
pub struct JjFixture {
    _directory: tempfile::TempDir,
    root: PathBuf,
}

#[derive(Debug)]
pub struct GitFixture {
    _directory: tempfile::TempDir,
    root: PathBuf,
}

impl GitFixture {
    pub fn new() -> Self {
        Self::new_in(&std::env::temp_dir())
    }

    fn new_in(parent: &Path) -> Self {
        let directory = fixture_directory(parent);
        let root = directory.path().join("repository");
        fs::create_dir(&root).unwrap();
        let fixture = Self {
            _directory: directory,
            root,
        };
        fixture.git(["init", "--quiet"]);
        fixture.git(["config", "user.name", "Reviewer Test"]);
        fixture.git(["config", "user.email", "reviewer@example.invalid"]);
        fixture.git(["commit", "--quiet", "--allow-empty", "-m", "base"]);
        fixture
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn commit_all(&self, message: &str) {
        self.git(["add", "-A"]);
        self.git(["commit", "--quiet", "-m", message]);
    }

    pub fn git<'a>(&self, arguments: impl IntoIterator<Item = &'a str>) -> Output {
        let output = Command::new("git")
            .args(arguments)
            .current_dir(&self.root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "Git fixture command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }
}

impl Default for GitFixture {
    fn default() -> Self {
        Self::new()
    }
}

impl ReviewRepositoryFixture for GitFixture {
    fn root(&self) -> &Path {
        self.root()
    }

    fn write(&self, relative_path: &str, contents: &[u8]) {
        let path = self.root().join(relative_path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn remove(&self, relative_path: &str) {
        fs::remove_file(self.root().join(relative_path)).unwrap();
    }

    fn rename(&self, from: &str, to: &str) {
        fs::rename(self.root().join(from), self.root().join(to)).unwrap();
    }

    fn symlink(&self, target: &str, link: &str) {
        std::os::unix::fs::symlink(target, self.root().join(link)).unwrap();
    }

    fn revision_id(&self) -> String {
        String::from_utf8(self.git(["rev-parse", "HEAD"]).stdout)
            .unwrap()
            .trim()
            .to_owned()
    }

    fn new_change(&self, description: &str) {
        self.commit_all(description);
    }

    fn edit(&self, revision: &str) {
        self.git(["reset", "--quiet", "--mixed", revision]);
    }

    fn rewrite_base_without_reviewed_file_change(&self) {
        self.git(["commit", "--quiet", "--amend", "-m", "rewritten base"]);
    }

    fn add_workspace(&self, name: &str) -> PathBuf {
        let root = self.root().with_file_name(name);
        self.git([
            "worktree",
            "add",
            "--quiet",
            "--detach",
            root.to_str().unwrap(),
        ]);
        root
    }
}

impl JjFixture {
    pub fn new(layout: JjLayout) -> Self {
        Self::new_in(layout, &std::env::temp_dir())
    }

    fn new_in(layout: JjLayout, parent: &Path) -> Self {
        let directory = fixture_directory(parent);
        let root = directory.path().join("repository");
        let layout_argument = match layout {
            JjLayout::NonColocated => "--no-colocate",
            JjLayout::Colocated => "--colocate",
        };
        let output = Command::new("jj")
            .args([
                OsString::from("--color=never"),
                OsString::from("--no-pager"),
                OsString::from("git"),
                OsString::from("init"),
                OsString::from(layout_argument),
                root.as_os_str().to_owned(),
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "jj fixture initialization must succeed"
        );

        Self {
            _directory: directory,
            root,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn write(&self, relative_path: impl AsRef<Path>, contents: &[u8]) {
        let relative_path = relative_path.as_ref();
        assert!(
            !relative_path.is_absolute()
                && relative_path
                    .components()
                    .all(|component| matches!(component, std::path::Component::Normal(_))),
            "fixture path must be a safe relative path"
        );
        let path = self.root.join(relative_path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    pub fn remove(&self, relative_path: impl AsRef<Path>) {
        fs::remove_file(self.root.join(relative_path)).unwrap();
    }

    pub fn rename(&self, from: impl AsRef<Path>, to: impl AsRef<Path>) {
        fs::rename(self.root.join(from), self.root.join(to)).unwrap();
    }

    pub fn symlink(&self, target: impl AsRef<Path>, link: impl AsRef<Path>) {
        std::os::unix::fs::symlink(target, self.root.join(link)).unwrap();
    }

    pub fn new_change(&self, description: &str) {
        self.jj(["new", "-m", description]);
    }

    pub fn edit(&self, revision: &str) {
        self.jj(["edit", revision]);
    }

    pub fn change_id(&self) -> String {
        let output = self.jj(["log", "--no-graph", "-r", "@", "-T", r#"change_id ++ "\n""#]);
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    pub fn add_workspace_on(&self, name: &str, parent: &str) -> PathBuf {
        let root = self.root.with_file_name(name);
        self.jj([
            "workspace",
            "add",
            "--name",
            name,
            "-r",
            parent,
            root.to_str().unwrap(),
        ]);
        root
    }

    pub fn working_copy(&self, workspace: &str) -> String {
        let revset = format!("{workspace}@");
        let output = self.jj([
            "log",
            "--no-graph",
            "-r",
            &revset,
            "-T",
            r#"change_id ++ "\n""#,
        ]);
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    pub fn jj<'a>(&self, arguments: impl IntoIterator<Item = &'a str>) -> Output {
        let output = Command::new("jj")
            .args(["--color=never", "--no-pager"])
            .args(arguments)
            .current_dir(&self.root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "jj fixture command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }
}

impl ReviewRepositoryFixture for JjFixture {
    fn root(&self) -> &Path {
        self.root()
    }

    fn write(&self, relative_path: &str, contents: &[u8]) {
        JjFixture::write(self, relative_path, contents);
    }

    fn remove(&self, relative_path: &str) {
        JjFixture::remove(self, relative_path);
    }

    fn rename(&self, from: &str, to: &str) {
        JjFixture::rename(self, from, to);
    }

    fn symlink(&self, target: &str, link: &str) {
        JjFixture::symlink(self, target, link);
    }

    fn revision_id(&self) -> String {
        self.change_id()
    }

    fn new_change(&self, description: &str) {
        JjFixture::new_change(self, description);
    }

    fn edit(&self, revision: &str) {
        JjFixture::edit(self, revision);
    }

    fn rewrite_base_without_reviewed_file_change(&self) {
        let reviewed_change = self.change_id();
        let old_parent = format!("{reviewed_change}-");
        self.jj(["new", &old_parent, "-m", "new parent"]);
        self.write("unrelated.txt", b"new parent\n");
        let new_parent = self.change_id();
        self.jj(["rebase", "-r", &reviewed_change, "-d", &new_parent]);
        self.edit(&reviewed_change);
    }

    fn add_workspace(&self, name: &str) -> PathBuf {
        let root = self.root().with_file_name(name);
        let change = self.change_id();
        let path = root.to_str().unwrap();
        self.jj(["workspace", "add", "--name", name, path]);
        let output = Command::new("jj")
            .args(["--color=never", "--no-pager", "edit", &change])
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "jj fixture command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        root
    }
}
