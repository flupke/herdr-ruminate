use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;

#[cfg(target_os = "linux")]
use nix::{
    errno::Errno,
    sys::signal::{Signal, killpg},
    sys::wait::{Id, WaitPidFlag, waitid},
    unistd::Pid,
};

use crate::{Error, Result};
use review_types::ReviewUnit;

mod comparison_plan;
mod evolution;
mod git;
mod jj;
mod watch;
mod workspaces;

pub use comparison_plan::{BaselineComparison, BaselineComparisonPlan, BaselineComparisonResults};
pub use evolution::{Candidate, ChangeEvolution};
pub use watch::{MetadataScope, MetadataWatch, WatchPlan};

const COMMAND_OUTPUT_LIMIT: usize = 256 * 1024 * 1024;

#[derive(Clone, Copy, Debug)]
struct RepositoryProcess<'a> {
    program: &'static str,
    options: &'static [&'static str],
    output_limit: usize,
    cwd: &'a Path,
    operation: &'static str,
    cancellation: &'a Cancellation,
    environment: &'a [(OsString, OsString)],
    input: &'a [u8],
}

#[derive(Clone, Debug, Default)]
struct Cancellation(Arc<CancellationSignal>);

#[derive(Debug, Default)]
struct CancellationSignal {
    cancelled: Mutex<bool>,
    changed: Condvar,
}

impl Cancellation {
    fn cancel(&self) {
        *self.lock() = true;
        self.0.changed.notify_all();
    }

    fn is_cancelled(&self) -> bool {
        *self.lock()
    }

    fn wake(&self) {
        let _cancelled = self.lock();
        self.0.changed.notify_all();
    }

    #[cfg(target_os = "linux")]
    fn wait_until(&self, settled: impl Fn() -> bool) -> bool {
        let mut cancelled = self.lock();
        while !*cancelled && !settled() {
            cancelled = self
                .0
                .changed
                .wait(cancelled)
                .unwrap_or_else(PoisonError::into_inner);
        }
        *cancelled
    }

    fn lock(&self) -> MutexGuard<'_, bool> {
        self.0
            .cancelled
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

#[derive(Debug, Default)]
struct CommandProgress {
    bytes: AtomicUsize,
    exceeded: AtomicBool,
    #[cfg(target_os = "linux")]
    exited: AtomicBool,
}

impl CommandProgress {
    fn exceeded(&self) -> bool {
        self.exceeded.load(Ordering::SeqCst)
    }

    #[cfg(target_os = "linux")]
    fn settled(&self) -> bool {
        self.exceeded() || self.exited.load(Ordering::SeqCst)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChangeId(ReviewUnit);

impl ChangeId {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn review_unit(&self) -> &ReviewUnit {
        &self.0
    }
}

impl From<String> for ChangeId {
    fn from(value: String) -> Self {
        Self(value.into())
    }
}

impl From<&ReviewUnit> for ChangeId {
    fn from(value: &ReviewUnit) -> Self {
        Self(value.clone())
    }
}

#[derive(
    Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
pub struct SnapshotId(String);

impl SnapshotId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for SnapshotId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

#[derive(
    Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
pub struct RepoPath(Vec<u8>);

impl RepoPath {
    pub fn from_bytes(bytes: impl Into<Vec<u8>>) -> Self {
        Self(bytes.into())
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    fn as_os_str(&self) -> &OsStr {
        OsStr::from_bytes(&self.0)
    }

    pub fn display(&self) -> String {
        match std::str::from_utf8(&self.0) {
            Ok(path) => {
                let mut display = String::with_capacity(path.len());
                for character in path.chars() {
                    if character.is_ascii() {
                        display.extend(std::ascii::escape_default(character as u8).map(char::from));
                    } else if character.is_control() {
                        display.extend(character.escape_default());
                    } else {
                        display.push(character);
                    }
                }
                display
            }
            Err(_) => self
                .0
                .iter()
                .flat_map(|byte| std::ascii::escape_default(*byte))
                .map(char::from)
                .collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum FileKind {
    Absent,
    File,
    Symlink,
    Conflict,
    Gitlink,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    TypeChanged,
    Conflict,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChangedFile {
    pub old_path: Option<RepoPath>,
    pub new_path: Option<RepoPath>,
    pub old_kind: FileKind,
    pub new_kind: FileKind,
    pub change: ChangeKind,
    pub display_path: String,
    pub statistics: DiffStatistics,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Serialize, ts_rs::TS)]
pub struct ShortRevision {
    pub prefix: String,
    pub rest: String,
}

impl ShortRevision {
    // jj colours the shortest prefix that names a revision apart from the rest of its
    // abbreviation; a Git abbreviation has one colour.
    fn of(display_id: &str) -> Self {
        let mut runs = Vec::new();
        let mut run = String::new();
        let mut characters = display_id.chars();
        while let Some(character) = characters.next() {
            if character == '\u{1b}' {
                // ECMA-48: a control sequence ends with its final byte, from `@` to `~`.
                if characters.next() == Some('[') {
                    characters.find(|character| ('@'..='~').contains(character));
                }
                if !run.is_empty() {
                    runs.push(std::mem::take(&mut run));
                }
            } else {
                run.push(character);
            }
        }
        if !run.is_empty() {
            runs.push(run);
        }
        if runs.len() < 2 {
            return Self {
                prefix: String::new(),
                rest: runs.concat(),
            };
        }
        let mut runs = runs.into_iter();
        Self {
            prefix: runs.next().unwrap_or_default(),
            rest: runs.collect(),
        }
    }
}

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize, ts_rs::TS,
)]
pub struct DiffStatistics {
    pub lines_added: u64,
    pub lines_removed: u64,
}

impl DiffStatistics {
    pub fn lines(&self) -> u64 {
        self.lines_added + self.lines_removed
    }

    pub fn from_unified_diff(diff: &[u8]) -> Self {
        let mut statistics = Self::default();
        let mut in_hunk = false;
        for line in diff.split(|byte| *byte == b'\n') {
            if line.starts_with(b"diff --git ") {
                in_hunk = false;
            } else if line.starts_with(b"@@ ") {
                in_hunk = true;
            } else if in_hunk && line.starts_with(b"+") {
                statistics.lines_added += 1;
            } else if in_hunk && line.starts_with(b"-") {
                statistics.lines_removed += 1;
            }
        }
        statistics
    }
}

impl ChangedFile {
    pub fn modified(path: impl AsRef<OsStr>) -> Self {
        let path = RepoPath::from_bytes(path.as_ref().as_bytes());
        Self {
            old_path: Some(path.clone()),
            new_path: Some(path.clone()),
            old_kind: FileKind::File,
            new_kind: FileKind::File,
            change: ChangeKind::Modified,
            display_path: path.display(),
            statistics: DiffStatistics::default(),
        }
    }

    /// # Panics
    ///
    /// Panics if both path fields are `None`.
    pub fn review_path(&self) -> &RepoPath {
        self.new_path
            .as_ref()
            .or(self.old_path.as_ref())
            .expect("a changed file always has a path")
    }

    pub fn changes_text_lines(&self) -> bool {
        self.statistics.lines() > 0
    }

    fn sort_by_review_path(files: &mut [Self]) {
        files.sort_by(|left, right| {
            left.review_path()
                .as_bytes()
                .cmp(right.review_path().as_bytes())
        });
    }

    fn display_path(old_path: Option<&RepoPath>, new_path: Option<&RepoPath>) -> Option<String> {
        match (old_path, new_path) {
            (Some(old), Some(new)) if old != new => {
                Some(format!("{} => {}", old.display(), new.display()))
            }
            (Some(path), _) | (_, Some(path)) => Some(path.display()),
            (None, None) => None,
        }
    }

    fn entry_type_changed(old_kind: FileKind, new_kind: FileKind) -> bool {
        old_kind != FileKind::Absent && new_kind != FileKind::Absent && old_kind != new_kind
    }

    fn diff_paths(&self) -> impl Iterator<Item = &RepoPath> {
        self.old_path.iter().chain(
            self.new_path
                .iter()
                .filter(|new_path| self.old_path.as_ref() != Some(*new_path)),
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SnapshotIdentity {
    Jj {
        change_id: ChangeId,
        snapshot_id: SnapshotId,
        description: String,
        display_id: String,
        #[serde(default)]
        names: Vec<String>,
    },
    Git {
        base_tree: ReviewUnit,
        display_id: String,
        snapshot_id: SnapshotId,
    },
}

impl SnapshotIdentity {
    pub fn review_unit(&self) -> &ReviewUnit {
        match self {
            Self::Jj { change_id, .. } => &change_id.0,
            Self::Git { base_tree, .. } => base_tree,
        }
    }

    pub fn snapshot_id(&self) -> &str {
        match self {
            Self::Jj { snapshot_id, .. } | Self::Git { snapshot_id, .. } => snapshot_id.as_str(),
        }
    }

    pub fn same_snapshot(&self, other: &Self) -> bool {
        self.review_unit() == other.review_unit() && self.snapshot_id() == other.snapshot_id()
    }

    pub fn display_id(&self) -> &str {
        match self {
            Self::Jj { display_id, .. } | Self::Git { display_id, .. } => display_id,
        }
    }

    pub fn short_revision(&self) -> ShortRevision {
        ShortRevision::of(self.display_id())
    }

    pub fn title(&self) -> &str {
        self.description().lines().next().unwrap_or_default()
    }

    pub fn change_description(&self) -> Option<&str> {
        match self {
            Self::Jj { description, .. } => {
                Some(description.trim()).filter(|description| !description.is_empty())
            }
            Self::Git { .. } => None,
        }
    }

    pub fn names(&self) -> &[String] {
        match self {
            Self::Jj { names, .. } => names,
            Self::Git { .. } => &[],
        }
    }

    pub fn description(&self) -> &str {
        match self {
            Self::Jj { description, .. } => description,
            Self::Git { .. } => "Git working tree\n",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Snapshot {
    pub identity: SnapshotIdentity,
    pub files: Vec<ChangedFile>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PollResult {
    Complete(Snapshot),
    ChangedDuringPoll,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Interdiff {
    MissingBaseline,
    Diff(Vec<u8>),
}

trait RepositoryBackend: std::fmt::Debug + Send + Sync {
    fn repo_type(&self) -> RepoType;
    fn set_state_root(&self, repository_root: &Path, state_root: &Path);
    fn watch_plan(&self, root: &Path) -> WatchPlan;
    fn current_identity(&self, repository: &Repository) -> Result<SnapshotIdentity>;
    fn identity_of(
        &self,
        repository: &Repository,
        change_id: &ChangeId,
    ) -> Result<Option<SnapshotIdentity>>;
    fn read_files(
        &self,
        repository: &Repository,
        identity: &SnapshotIdentity,
    ) -> Result<Vec<ChangedFile>>;
    fn read_stats(
        &self,
        repository: &Repository,
        identity: &SnapshotIdentity,
        files: &mut [ChangedFile],
    ) -> Result<()>;
    fn diff(
        &self,
        repository: &Repository,
        snapshot: &Snapshot,
        file: &ChangedFile,
    ) -> Result<Vec<u8>>;
    fn file_at(
        &self,
        repository: &Repository,
        revision: &str,
        path: &RepoPath,
    ) -> Result<Option<Vec<u8>>>;
    fn base_file_at(
        &self,
        repository: &Repository,
        snapshot: &Snapshot,
        path: &RepoPath,
    ) -> Result<Vec<u8>>;
    fn interdiff(
        &self,
        repository: &Repository,
        baseline_snapshot_id: &str,
        snapshot: &Snapshot,
        path: &RepoPath,
    ) -> Result<Interdiff>;
    fn compare_baselines(
        &self,
        repository: &Repository,
        snapshot: &Snapshot,
        plan: &BaselineComparisonPlan,
    ) -> Result<BaselineComparisonResults>;
    fn revision_candidates(
        &self,
        repository: &Repository,
        direction: RevisionDirection,
    ) -> Result<Vec<RevisionCandidate>>;
    fn revision_history(&self, repository: &Repository) -> Result<Vec<RevisionHistoryLine>>;
    fn edit_revision(&self, repository: &Repository, change_id: &ChangeId) -> Result<RevisionEdit>;
    fn evolution(
        &self,
        repository: &Repository,
        identity: &SnapshotIdentity,
        known: &dyn Fn(&str) -> Option<Arc<ChangeEvolution>>,
    ) -> Result<ChangeEvolution>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::Display, strum::EnumString)]
#[strum(serialize_all = "lowercase")]
pub enum RepoType {
    Git,
    Jj,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RevisionDirection {
    Parents,
    Children,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RevisionEdit {
    Edited,
    Unavailable,
    OtherWorkspace { workspaces: Vec<String> },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RevisionCandidate {
    pub change_id: ChangeId,
    pub short_change_id: String,
    pub description: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RevisionHistoryLine {
    pub text: String,
    pub plain_text: String,
    pub graph_end: Option<usize>,
    pub short_change_id: Option<String>,
    pub change_id: Option<ChangeId>,
    pub commit_id: Option<SnapshotId>,
    pub is_current: bool,
    pub is_immutable: bool,
    pub refs: Vec<RevisionRef>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RevisionRef {
    pub kind: RevisionRefKind,
    pub name: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RevisionRefKind {
    Bookmark,
    Tag,
}

#[derive(Clone, Debug)]
pub struct Repository {
    root: PathBuf,
    backend: Arc<dyn RepositoryBackend>,
    cancellation: Cancellation,
}

impl Repository {
    pub fn discover(start: impl AsRef<Path>) -> Result<Self> {
        let start = start.as_ref();
        let cancellation = Cancellation::default();
        let jj_root = jj::JjBackend::workspace_root(start, &cancellation)?;
        let git_root = git::GitBackend::working_tree_root(start, &cancellation)?;

        if let Some(root) = jj_root
            && git_root
                .as_ref()
                .is_none_or(|git_root| root.components().count() >= git_root.components().count())
        {
            return Ok(Self {
                root,
                backend: Arc::new(jj::JjBackend::default()),
                cancellation,
            });
        }

        let Some(root) = git_root else {
            return Err(Error::NotRepository {
                path: start.to_owned(),
            });
        };
        let backend = git::GitBackend::discover(&root, &cancellation)?;
        Ok(Self {
            root,
            backend: Arc::new(backend),
            cancellation,
        })
    }

    #[must_use]
    pub fn with_state_root(self, state_root: impl AsRef<Path>) -> Self {
        self.backend.set_state_root(&self.root, state_root.as_ref());
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn repo_type(&self) -> RepoType {
        self.backend.repo_type()
    }

    pub fn watch_plan(&self) -> WatchPlan {
        self.backend.watch_plan(&self.root)
    }

    pub fn revision_candidates(
        &self,
        direction: RevisionDirection,
    ) -> Result<Vec<RevisionCandidate>> {
        self.backend.revision_candidates(self, direction)
    }

    pub fn revision_history(&self) -> Result<Vec<RevisionHistoryLine>> {
        self.backend.revision_history(self)
    }

    pub fn edit_revision(&self, change_id: &ChangeId) -> Result<RevisionEdit> {
        self.backend.edit_revision(self, change_id)
    }

    pub fn cancel(&self) {
        self.cancellation.cancel();
    }

    #[must_use]
    pub fn without_cancellation(&self) -> Self {
        Self {
            root: self.root.clone(),
            backend: Arc::clone(&self.backend),
            cancellation: Cancellation::default(),
        }
    }

    pub fn poll(&self) -> Result<PollResult> {
        let identity = self.current_identity()?;
        let mut files = self.backend.read_files(self, &identity)?;
        self.backend.read_stats(self, &identity, &mut files)?;
        let verified = self.current_identity()?;

        if !identity.same_snapshot(&verified) {
            return Ok(PollResult::ChangedDuringPoll);
        }

        Ok(PollResult::Complete(Snapshot {
            identity: verified,
            files,
        }))
    }

    pub fn snapshot_of(&self, change_id: &ChangeId) -> Result<Option<Snapshot>> {
        let Some(identity) = self.backend.identity_of(self, change_id)? else {
            return Ok(None);
        };
        let mut files = self.backend.read_files(self, &identity)?;
        self.backend.read_stats(self, &identity, &mut files)?;
        Ok(Some(Snapshot { identity, files }))
    }

    pub fn current_identity(&self) -> Result<SnapshotIdentity> {
        self.backend.current_identity(self)
    }

    pub fn evolution(
        &self,
        identity: &SnapshotIdentity,
        known: &dyn Fn(&str) -> Option<Arc<ChangeEvolution>>,
    ) -> Result<ChangeEvolution> {
        self.backend.evolution(self, identity, known)
    }

    pub fn diff(&self, snapshot: &Snapshot, file: &ChangedFile) -> Result<Vec<u8>> {
        self.backend.diff(self, snapshot, file)
    }

    pub fn file_at(&self, revision: &str, path: &RepoPath) -> Result<Option<Vec<u8>>> {
        self.backend.file_at(self, revision, path)
    }

    pub fn base_file_at(&self, snapshot: &Snapshot, path: &RepoPath) -> Result<Vec<u8>> {
        self.backend.base_file_at(self, snapshot, path)
    }

    pub fn interdiff(
        &self,
        baseline_snapshot_id: &str,
        snapshot: &Snapshot,
        path: &RepoPath,
    ) -> Result<Interdiff> {
        self.backend
            .interdiff(self, baseline_snapshot_id, snapshot, path)
    }

    pub fn compare_baselines(
        &self,
        snapshot: &Snapshot,
        plan: &BaselineComparisonPlan,
    ) -> Result<BaselineComparisonResults> {
        self.backend.compare_baselines(self, snapshot, plan)
    }
}

impl<'a> RepositoryProcess<'a> {
    fn new(
        program: &'static str,
        cwd: &'a Path,
        operation: &'static str,
        cancellation: &'a Cancellation,
    ) -> Self {
        Self {
            program,
            options: if program == "jj" {
                &["--color=never", "--no-pager"]
            } else {
                &["--no-pager", "-c", "color.ui=false"]
            },
            output_limit: COMMAND_OUTPUT_LIMIT,
            cwd,
            operation,
            cancellation,
            environment: &[],
            input: &[],
        }
    }

    fn with_input(mut self, input: &'a [u8]) -> Self {
        self.input = input;
        self
    }

    fn with_environment(mut self, environment: &'a [(OsString, OsString)]) -> Self {
        self.environment = environment;
        self
    }

    fn output<I, S>(self, arguments: I) -> Result<Output>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        if self.cancellation.is_cancelled() {
            return Err(Error::CommandCancelled {
                operation: self.operation.to_owned(),
                path: self.cwd.to_owned(),
            });
        }
        let mut child = Command::new(self.program)
            .args(self.options)
            .args(arguments)
            .envs(self.environment.iter().cloned())
            .current_dir(self.cwd)
            .stdin(if self.input.is_empty() {
                Stdio::null()
            } else {
                Stdio::piped()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()
            .map_err(|source| Error::Spawn {
                operation: self.operation.to_owned(),
                program: OsString::from(self.program),
                current_dir: Some(self.cwd.to_owned()),
                source,
            })?;
        if let Some(mut stdin) = child.stdin.take() {
            let input = self.input.to_vec();
            // POSIX: a child that exits before it reads all its input closes the pipe, and the
            // write fails with EPIPE.
            thread::spawn(move || {
                let _ = stdin.write_all(&input);
            });
        }
        let progress = Arc::<CommandProgress>::default();
        let stdout = self.capture(
            child.stdout.take().expect("stdout is piped"),
            Arc::clone(&progress),
        );
        let stderr = self.capture(
            child.stderr.take().expect("stderr is piped"),
            Arc::clone(&progress),
        );
        let (status, cancelled) = self.wait(&mut child, &progress);
        let status = status.map_err(|source| Error::Spawn {
            operation: self.operation.to_owned(),
            program: OsString::from(self.program),
            current_dir: Some(self.cwd.to_owned()),
            source,
        })?;
        let stdout = stdout
            .join()
            .expect("output reader did not panic")
            .map_err(|source| Error::Spawn {
                operation: self.operation.to_owned(),
                program: OsString::from(self.program),
                current_dir: Some(self.cwd.to_owned()),
                source,
            })?;
        let stderr = stderr
            .join()
            .expect("output reader did not panic")
            .map_err(|source| Error::Spawn {
                operation: self.operation.to_owned(),
                program: OsString::from(self.program),
                current_dir: Some(self.cwd.to_owned()),
                source,
            })?;
        if cancelled {
            return Err(Error::CommandCancelled {
                operation: self.operation.to_owned(),
                path: self.cwd.to_owned(),
            });
        }
        if progress.exceeded() {
            return Err(Error::CommandOutputTooLarge {
                operation: self.operation.to_owned(),
                path: self.cwd.to_owned(),
            });
        }
        Ok(Output {
            status,
            stdout,
            stderr,
        })
    }

    #[cfg(target_os = "linux")]
    fn wait(
        &self,
        child: &mut Child,
        progress: &Arc<CommandProgress>,
    ) -> (std::io::Result<ExitStatus>, bool) {
        let exit = Self::watch_exit(child, Arc::clone(progress), self.cancellation.clone());
        let cancelled = self.cancellation.wait_until(|| progress.settled()) && !progress.exceeded();
        if cancelled || progress.exceeded() {
            if let Ok(pid) = i32::try_from(child.id()) {
                let _ = killpg(Pid::from_raw(pid), Signal::SIGKILL);
            }
            let _ = child.kill();
        }
        // `waitid` with `WNOWAIT` leaves the child unreaped: its process ID stays the child's until
        // `wait` below.
        exit.join().expect("exit watcher did not panic");
        (child.wait(), cancelled)
    }

    // Outside Linux, no `waitid` wakes the command when its child exits.
    #[cfg(not(target_os = "linux"))]
    fn wait(
        &self,
        child: &mut Child,
        progress: &Arc<CommandProgress>,
    ) -> (std::io::Result<ExitStatus>, bool) {
        loop {
            if progress.exceeded() {
                let _ = child.kill();
                return (child.wait(), false);
            }
            if self.cancellation.is_cancelled() {
                let _ = child.kill();
                return (child.wait(), true);
            }
            match child.try_wait() {
                Ok(Some(status)) => return (Ok(status), false),
                Ok(None) => thread::sleep(std::time::Duration::from_millis(10)),
                Err(error) => return (Err(error), false),
            }
        }
    }

    fn capture(
        &self,
        mut pipe: impl Read + Send + 'static,
        progress: Arc<CommandProgress>,
    ) -> thread::JoinHandle<std::io::Result<Vec<u8>>> {
        let cancellation = self.cancellation.clone();
        let limit = self.output_limit;
        thread::spawn(move || {
            let mut output = Vec::new();
            let mut buffer = [0; 8192];
            loop {
                let count = pipe.read(&mut buffer)?;
                if count == 0 {
                    return Ok(output);
                }
                let previous = progress.bytes.fetch_add(count, Ordering::Relaxed);
                if previous.saturating_add(count) > limit {
                    progress.exceeded.store(true, Ordering::SeqCst);
                    cancellation.wake();
                    return Ok(output);
                }
                output.extend_from_slice(&buffer[..count]);
            }
        })
    }

    #[cfg(target_os = "linux")]
    fn watch_exit(
        child: &Child,
        progress: Arc<CommandProgress>,
        cancellation: Cancellation,
    ) -> thread::JoinHandle<()> {
        let pid = Pid::from_raw(i32::try_from(child.id()).expect("a process ID fits an i32"));
        thread::spawn(move || {
            // `waitid` fails with `EINTR` when interrupted; any other error means the child cannot be
            // waited for.
            while let Err(Errno::EINTR) =
                waitid(Id::Pid(pid), WaitPidFlag::WEXITED | WaitPidFlag::WNOWAIT)
            {}
            progress.exited.store(true, Ordering::SeqCst);
            cancellation.wake();
        })
    }
}

fn path_from_output(
    output: &[u8],
    operation: &'static str,
    empty_detail: &'static str,
) -> Result<PathBuf> {
    let path = trim_line_ending(output);
    if path.is_empty() {
        return Err(Error::Protocol {
            operation: operation.to_owned(),
            detail: empty_detail,
        });
    }
    Ok(PathBuf::from(OsString::from_vec(path.to_vec())))
}

fn trim_line_ending(bytes: &[u8]) -> &[u8] {
    bytes
        .strip_suffix(b"\r\n")
        .or_else(|| bytes.strip_suffix(b"\n"))
        .unwrap_or(bytes)
}

#[cfg(test)]
#[path = "repository.tests.rs"]
mod tests;
