#[path = "carried_marks.rs"]
mod carried_marks;
#[path = "diff_cache.rs"]
mod diff_cache;

use std::path::Path;
use std::sync::{Arc, Mutex};

use diff_cache::DiffCache;
use review_hunks::{
    Attribution, ChangedLines, FileHunks, HunkMark, HunkReview, HunkSpan, LineCount, LineSelection,
    MarkAge, Reviewed, ReviewedLines, ReviewedVersion, replay, reverse_apply,
};
use review_repository::diff::parse_file_diff;
use review_repository::repository::{
    BaselineComparison, BaselineComparisonPlan, ChangedFile, DiffStatistics, FileKind, Interdiff,
    RepoPath, Repository, Snapshot, SnapshotId,
};
use review_store::{LoadResult, PartialReview, ReviewRecord, ReviewStore};
use review_types::{MarkAuthor, ReviewUnit};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewStatus {
    Unreviewed,
    Reviewed,
    ChangedSinceReview,
    PartiallyReviewed,
}

impl ReviewStatus {
    pub fn needs_review(self) -> bool {
        self != Self::Reviewed
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewWarning {
    UnknownSchema,
    BaselineExpired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReviewState {
    pub status: ReviewStatus,
    pub warning: Option<ReviewWarning>,
    pub current_diff_statistics: DiffStatistics,
    pub lines: Option<LineCount>,
}

impl ReviewState {
    pub fn unreviewed(
        current_diff_statistics: DiffStatistics,
        warning: Option<ReviewWarning>,
    ) -> Self {
        Self {
            status: ReviewStatus::Unreviewed,
            warning,
            current_diff_statistics,
            lines: None,
        }
    }

    pub fn reviewed() -> Self {
        Self {
            status: ReviewStatus::Reviewed,
            warning: None,
            current_diff_statistics: DiffStatistics::default(),
            lines: None,
        }
    }

    pub fn changed_since_review(current_diff_statistics: DiffStatistics) -> Self {
        Self {
            status: ReviewStatus::ChangedSinceReview,
            warning: None,
            current_diff_statistics,
            lines: None,
        }
    }

    pub fn partially_reviewed(
        current_diff_statistics: DiffStatistics,
        lines: Option<LineCount>,
    ) -> Self {
        Self {
            status: ReviewStatus::PartiallyReviewed,
            warning: None,
            current_diff_statistics,
            lines,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewDiff {
    pub unified: Vec<u8>,
    pub old_content: Option<Vec<u8>>,
    pub new_content: Option<Vec<u8>>,
    pub hunks: FileHunks,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FileLines {
    pub open: Vec<OpenLines>,
    pub reviewed: ReviewedLines,
}

impl FileLines {
    pub fn open_selection(&self) -> LineSelection {
        let mut selection = LineSelection::default();
        for open in &self.open {
            selection.removed.extend(&open.lines.removed);
            selection.added.extend(&open.lines.added);
        }
        selection
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenLines {
    pub lines: ChangedLines,
    pub since_review: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarkResult {
    Marked,
    ChangeChanged,
    NothingToMark,
}

#[derive(Debug, Eq, PartialEq)]
enum ReviewComparison {
    Unreviewed(Option<ReviewWarning>),
    Compared {
        baseline_commit_id: String,
        diff: Vec<u8>,
    },
    Partial {
        reviewed: Reviewed,
        current: Option<Vec<u8>>,
        diff: Vec<u8>,
        hunks: FileHunks,
    },
}

enum PlannedReviewState {
    Resolved(ReviewState),
    Compare { baseline_snapshot_id: SnapshotId },
}

impl ReviewComparison {
    fn state(&self, file: &ChangedFile) -> ReviewState {
        match self {
            Self::Unreviewed(warning) => ReviewState::unreviewed(file.statistics, *warning),
            Self::Compared { diff, .. } | Self::Partial { diff, .. } if diff.is_empty() => {
                ReviewState::reviewed()
            }
            Self::Compared { diff, .. } => {
                ReviewState::changed_since_review(DiffStatistics::from_unified_diff(diff))
            }
            Self::Partial { diff, hunks, .. } => ReviewState::partially_reviewed(
                DiffStatistics::from_unified_diff(diff),
                hunks.count(),
            ),
        }
    }
}

#[derive(Debug)]
pub struct ReviewTracker {
    repository: Repository,
    store: ReviewStore,
    diffs: Mutex<DiffCache>,
    evolutions: Arc<carried_marks::Evolutions>,
    fruitless: carried_marks::Fruitless,
}

impl ReviewTracker {
    pub fn new(repository: Repository, store: ReviewStore) -> Self {
        Self {
            repository,
            store,
            diffs: Mutex::new(DiffCache::default()),
            evolutions: Arc::default(),
            fruitless: carried_marks::Fruitless::default(),
        }
    }

    pub fn copy_into(&self, review_unit: &ReviewUnit, state_root: &Path) -> eyre::Result<Self> {
        Ok(Self {
            evolutions: Arc::clone(&self.evolutions),
            ..Self::new(
                self.repository.clone(),
                self.store.copy_marks_to(review_unit, state_root)?,
            )
        })
    }

    pub fn mark(
        &self,
        snapshot: &Snapshot,
        file: &ChangedFile,
        author: &MarkAuthor,
    ) -> eyre::Result<MarkResult> {
        let identity = self.repository.current_identity()?;
        if identity.review_unit() != snapshot.identity.review_unit() {
            return Ok(MarkResult::ChangeChanged);
        }
        let _lock = self.store.lock_marks(identity.review_unit())?;
        self.store.mark(
            identity.review_unit(),
            file.review_path().as_bytes(),
            identity.snapshot_id(),
            author,
        )?;
        Ok(MarkResult::Marked)
    }

    pub fn status(&self, snapshot: &Snapshot, file: &ChangedFile) -> eyre::Result<ReviewState> {
        Ok(self.compare(snapshot, file)?.state(file))
    }

    pub fn statuses(&self, snapshot: &Snapshot) -> eyre::Result<Vec<ReviewState>> {
        self.take_origin_marks(snapshot);
        let review_unit = snapshot.identity.review_unit();
        let mut plan = BaselineComparisonPlan::default();
        let mut planned_states = Vec::with_capacity(snapshot.files.len());
        for file in &snapshot.files {
            planned_states.push(self.plan_state(snapshot, file, &mut plan)?);
        }

        let results = self.repository.compare_baselines(snapshot, &plan)?;
        snapshot
            .files
            .iter()
            .zip(planned_states)
            .map(|(file, planned_state)| match planned_state {
                PlannedReviewState::Resolved(state) => Ok(state),
                PlannedReviewState::Compare {
                    baseline_snapshot_id,
                } => match results.get(&baseline_snapshot_id) {
                    Some(BaselineComparison::Missing) => {
                        self.store
                            .unreview(review_unit, file.review_path().as_bytes())?;
                        Ok(ReviewState::unreviewed(
                            file.statistics,
                            Some(ReviewWarning::BaselineExpired),
                        ))
                    }
                    Some(BaselineComparison::Compared { path_statistics }) => {
                        Ok(match path_statistics.get(file.review_path()) {
                            Some(statistics) => ReviewState::changed_since_review(*statistics),
                            None => ReviewState::reviewed(),
                        })
                    }
                    None => Err(eyre::eyre!("comparison plan omitted a stored baseline")),
                },
            })
            .collect()
    }

    fn plan_state(
        &self,
        snapshot: &Snapshot,
        file: &ChangedFile,
        plan: &mut BaselineComparisonPlan,
    ) -> eyre::Result<PlannedReviewState> {
        let review_unit = snapshot.identity.review_unit();
        Ok(
            match self
                .store
                .load(review_unit, file.review_path().as_bytes())?
            {
                LoadResult::Unreviewed => {
                    PlannedReviewState::Resolved(ReviewState::unreviewed(file.statistics, None))
                }
                LoadResult::UnknownSchema => PlannedReviewState::Resolved(ReviewState::unreviewed(
                    file.statistics,
                    Some(ReviewWarning::UnknownSchema),
                )),
                LoadResult::Reviewed(ReviewRecord {
                    partial: Some(partial),
                    baseline_commit_id,
                    ..
                }) => PlannedReviewState::Resolved(
                    self.compare_partial(snapshot, file, &partial, &baseline_commit_id)?
                        .state(file),
                ),
                LoadResult::Reviewed(record) => {
                    let baseline_snapshot_id = SnapshotId::from(record.baseline_commit_id);
                    plan.add(baseline_snapshot_id.clone(), file.review_path().clone());
                    PlannedReviewState::Compare {
                        baseline_snapshot_id,
                    }
                }
            },
        )
    }

    pub fn mark_hunk(
        &self,
        snapshot: &Snapshot,
        file: &ChangedFile,
        mark: &HunkMark,
        author: &MarkAuthor,
    ) -> eyre::Result<MarkResult> {
        self.edit(snapshot, file, |diff, review| {
            let listed = match mark {
                HunkMark::Review(span) => diff.hunks.open.iter().any(|hunk| &hunk.span == span),
                HunkMark::Unreview(span) => {
                    diff.hunks.reviewed.iter().any(|hunk| &hunk.span == span)
                }
            };
            eyre::ensure!(listed, "the hunk changed; wait for the next refresh");
            let version = match mark {
                HunkMark::Review(span) => review.review(span, author),
                HunkMark::Unreview(span) => review.unreview(span),
            }
            .ok_or_else(|| {
                eyre::eyre!("the hunk no longer matches the file; wait for the next refresh")
            })?;
            Ok(Some(version))
        })
    }

    pub fn whole_file_author(
        &self,
        snapshot: &Snapshot,
        file: &ChangedFile,
    ) -> eyre::Result<Option<MarkAuthor>> {
        Ok(
            match self.store.load(
                snapshot.identity.review_unit(),
                file.review_path().as_bytes(),
            )? {
                LoadResult::Reviewed(record) if record.partial.is_none() => Some(record.author),
                _ => None,
            },
        )
    }

    pub fn lines(&self, snapshot: &Snapshot, file: &ChangedFile) -> eyre::Result<FileLines> {
        if !file.changes_text_lines() {
            return Ok(FileLines::default());
        }
        let diff = self.diff(snapshot, file)?;
        if diff.hunks.is_empty() {
            return Ok(FileLines::default());
        }
        let versions = self.versions(snapshot, file, &diff)?;
        let review = versions.review();
        let spans = diff
            .hunks
            .open
            .iter()
            .map(|hunk| hunk.span.clone())
            .collect::<Vec<HunkSpan>>();
        Ok(FileLines {
            open: review
                .changed_lines(&spans)
                .into_iter()
                .zip(&diff.hunks.open)
                .map(|(lines, hunk)| OpenLines {
                    lines,
                    since_review: hunk.since_review,
                })
                .collect(),
            reviewed: review.reviewed_lines(),
        })
    }

    pub fn accept_lines(
        &self,
        snapshot: &Snapshot,
        file: &ChangedFile,
        selection: &LineSelection,
        author: &MarkAuthor,
    ) -> eyre::Result<MarkResult> {
        self.edit(snapshot, file, |diff, review| {
            Ok((!diff.hunks.is_empty())
                .then(|| review.accept_lines(selection, author))
                .flatten())
        })
    }

    pub fn reopen_lines(
        &self,
        snapshot: &Snapshot,
        file: &ChangedFile,
        selection: &LineSelection,
    ) -> eyre::Result<MarkResult> {
        self.edit(snapshot, file, |diff, review| {
            Ok((!diff.hunks.is_empty())
                .then(|| review.reopen_lines(selection))
                .flatten())
        })
    }

    fn edit(
        &self,
        snapshot: &Snapshot,
        file: &ChangedFile,
        edit: impl FnOnce(&ReviewDiff, &HunkReview<'_>) -> eyre::Result<Option<ReviewedVersion>>,
    ) -> eyre::Result<MarkResult> {
        let identity = self.repository.current_identity()?;
        if identity.review_unit() != snapshot.identity.review_unit() {
            return Ok(MarkResult::ChangeChanged);
        }
        let _lock = self.store.lock_marks(identity.review_unit())?;
        let diff = self.diff(snapshot, file)?;
        let versions = self.versions(snapshot, file, &diff)?;
        let Some(version) = edit(&diff, &versions.review())? else {
            return Ok(MarkResult::NothingToMark);
        };
        self.store_version(snapshot, file, &versions, version)?;
        Ok(MarkResult::Marked)
    }

    pub fn review_hunks_where(
        &self,
        snapshot: &Snapshot,
        file: &ChangedFile,
        may_review_file: bool,
        author: &MarkAuthor,
        accept: impl Fn(&ChangedLines) -> bool,
    ) -> eyre::Result<usize> {
        let _lock = self.store.lock_marks(snapshot.identity.review_unit())?;
        let diff = self.diff(snapshot, file)?;
        let spans = diff
            .hunks
            .open
            .iter()
            .map(|hunk| hunk.span.clone())
            .collect::<Vec<_>>();
        if spans.is_empty() {
            return Ok(0);
        }
        let versions = self.versions(snapshot, file, &diff)?;
        let review = versions.review();
        let accepted = spans
            .iter()
            .zip(review.changed_lines(&spans))
            .filter(|(_, lines)| accept(lines))
            .map(|(span, _)| span.clone())
            .collect::<Vec<_>>();
        if accepted.is_empty() || (!may_review_file && accepted.len() == spans.len()) {
            return Ok(0);
        }
        let version = review
            .review_all(&accepted, author)
            .ok_or_else(|| eyre::eyre!("the open hunks overlap"))?;
        self.store_version(snapshot, file, &versions, version)?;
        Ok(accepted.len())
    }

    pub(crate) fn versions(
        &self,
        snapshot: &Snapshot,
        file: &ChangedFile,
        diff: &ReviewDiff,
    ) -> eyre::Result<Versions> {
        let base = self.base_text(snapshot, file)?;
        let reviewed = self.reviewed_version(snapshot, file, diff, &base)?;
        Ok(Versions {
            base,
            reviewed,
            current: diff.new_content.clone().unwrap_or_default(),
        })
    }

    fn store_version(
        &self,
        snapshot: &Snapshot,
        file: &ChangedFile,
        versions: &Versions,
        version: ReviewedVersion,
    ) -> eyre::Result<()> {
        let review_unit = snapshot.identity.review_unit();
        let path = file.review_path().as_bytes();
        let snapshot_id = snapshot.identity.snapshot_id();
        let reviewed = match version {
            ReviewedVersion::Current(attribution) => match attribution.uniform_author() {
                Some(author) => {
                    self.store.mark(review_unit, path, snapshot_id, author)?;
                    return Ok(());
                }
                None => Reviewed {
                    text: versions.current.clone(),
                    attribution,
                },
            },
            ReviewedVersion::Base => {
                self.store.unreview(review_unit, path)?;
                return Ok(());
            }
            ReviewedVersion::Partial(reviewed) => reviewed,
        };
        self.store.mark_partial(
            review_unit,
            path,
            snapshot_id,
            PartialReview {
                base: versions.base.clone(),
                reviewed,
            },
        )?;
        Ok(())
    }

    fn reviewed_version(
        &self,
        snapshot: &Snapshot,
        file: &ChangedFile,
        diff: &ReviewDiff,
        base: &[u8],
    ) -> eyre::Result<Reviewed> {
        let record = self.store.load(
            snapshot.identity.review_unit(),
            file.review_path().as_bytes(),
        )?;
        match record {
            LoadResult::Unreviewed => Ok(Reviewed {
                text: base.to_vec(),
                attribution: Attribution::default(),
            }),
            LoadResult::UnknownSchema => Err(eyre::eyre!(
                "a newer reviewer wrote this file's review mark"
            )),
            LoadResult::Reviewed(ReviewRecord {
                partial: Some(partial),
                ..
            }) => Ok(replay(&partial.base, &partial.reviewed, base)),
            LoadResult::Reviewed(record) => Ok(Reviewed {
                text: whole_file_reviewed_version(diff).ok_or_else(|| {
                    eyre::eyre!("could not rebuild the reviewed version of the file")
                })?,
                attribution: Attribution::uniform(record.author),
            }),
        }
    }

    fn compare_partial(
        &self,
        snapshot: &Snapshot,
        file: &ChangedFile,
        partial: &PartialReview,
        baseline: &str,
    ) -> eyre::Result<ReviewComparison> {
        let base = self.base_text(snapshot, file)?;
        let reviewed = replay(&partial.base, &partial.reviewed, &base);
        if reviewed.text == base {
            return Ok(ReviewComparison::Unreviewed(None));
        }
        let current = self.file_content(
            snapshot.identity.snapshot_id(),
            file.new_path.as_ref(),
            file.new_kind,
        )?;
        let current_text = current.as_deref().unwrap_or_default();
        let diff =
            review_hunks::unified_diff(&file.review_path().display(), &reviewed.text, current_text);
        let age = self.mark_age(file, baseline, snapshot, current.as_deref());
        let hunks = HunkReview::new(&base, &reviewed.text, current_text)
            .hunks(&parse_file_diff(&diff, file), age);
        Ok(ReviewComparison::Partial {
            reviewed,
            current,
            diff,
            hunks,
        })
    }

    fn mark_age(
        &self,
        file: &ChangedFile,
        baseline: &str,
        snapshot: &Snapshot,
        current: Option<&[u8]>,
    ) -> MarkAge {
        if baseline == snapshot.identity.snapshot_id() {
            return MarkAge::Current;
        }
        match self.file_content(baseline, file.new_path.as_ref(), file.new_kind) {
            Ok(marked) if marked.as_deref() == current => MarkAge::Current,
            _ => MarkAge::Outdated,
        }
    }

    fn base_text(&self, snapshot: &Snapshot, file: &ChangedFile) -> eyre::Result<Vec<u8>> {
        Ok(self
            .base_file_content(snapshot, file.old_path.as_ref(), file.old_kind)?
            .unwrap_or_default())
    }

    pub fn diff(&self, snapshot: &Snapshot, file: &ChangedFile) -> eyre::Result<ReviewDiff> {
        let record = self.store.load(
            snapshot.identity.review_unit(),
            file.review_path().as_bytes(),
        )?;
        if let Some(diff) = self
            .diffs
            .lock()
            .map_err(|_| eyre::eyre!("diff cache lock poisoned"))?
            .get(&snapshot.identity, file.review_path(), &record)
        {
            return Ok(diff);
        }
        let diff = self.load_diff(snapshot, file, record.clone())?;
        self.diffs
            .lock()
            .map_err(|_| eyre::eyre!("diff cache lock poisoned"))?
            .insert(
                &snapshot.identity,
                file.review_path().clone(),
                record,
                &diff,
            );
        Ok(diff)
    }

    fn load_diff(
        &self,
        snapshot: &Snapshot,
        file: &ChangedFile,
        record: LoadResult,
    ) -> eyre::Result<ReviewDiff> {
        match self.compare_record(snapshot, file, record)? {
            ReviewComparison::Unreviewed(_) => {
                let commit_id = snapshot.identity.snapshot_id();
                let unified = self.repository.diff(snapshot, file)?;
                Ok(ReviewDiff {
                    hunks: FileHunks::unreviewed(&parse_file_diff(&unified, file)),
                    unified,
                    old_content: self.base_file_content(
                        snapshot,
                        file.old_path.as_ref(),
                        file.old_kind,
                    )?,
                    new_content: self.file_content(
                        commit_id,
                        file.new_path.as_ref(),
                        file.new_kind,
                    )?,
                })
            }
            ReviewComparison::Compared {
                baseline_commit_id,
                diff,
            } => {
                let kind = if file.old_kind == FileKind::File {
                    file.old_kind
                } else {
                    file.new_kind
                };
                let mut loaded = ReviewDiff {
                    unified: diff,
                    old_content: self.file_content(
                        &baseline_commit_id,
                        Some(file.review_path()),
                        kind,
                    )?,
                    new_content: self.file_content(
                        snapshot.identity.snapshot_id(),
                        file.new_path.as_ref(),
                        file.new_kind,
                    )?,
                    hunks: FileHunks::default(),
                };
                if let Some(reviewed) = whole_file_reviewed_version(&loaded) {
                    let base = self.base_text(snapshot, file)?;
                    let current = loaded.new_content.as_deref().unwrap_or_default();
                    loaded.hunks = HunkReview::new(&base, &reviewed, current)
                        .hunks(&parse_file_diff(&loaded.unified, file), MarkAge::Outdated);
                }
                Ok(loaded)
            }
            ReviewComparison::Partial {
                reviewed,
                current,
                diff,
                hunks,
            } => Ok(ReviewDiff {
                unified: diff,
                old_content: Some(reviewed.text),
                new_content: current,
                hunks,
            }),
        }
    }

    fn file_content(
        &self,
        revision: &str,
        path: Option<&RepoPath>,
        kind: FileKind,
    ) -> eyre::Result<Option<Vec<u8>>> {
        if kind != FileKind::File {
            return Ok(None);
        }
        let Some(path) = path else {
            return Ok(None);
        };
        Ok(self.repository.file_at(revision, path)?)
    }

    fn base_file_content(
        &self,
        snapshot: &Snapshot,
        path: Option<&RepoPath>,
        kind: FileKind,
    ) -> eyre::Result<Option<Vec<u8>>> {
        if kind != FileKind::File {
            return Ok(None);
        }
        path.map(|path| self.repository.base_file_at(snapshot, path))
            .transpose()
            .map_err(Into::into)
    }

    fn compare(&self, snapshot: &Snapshot, file: &ChangedFile) -> eyre::Result<ReviewComparison> {
        let review_unit = snapshot.identity.review_unit();
        let path = file.review_path().as_bytes();
        self.compare_record(snapshot, file, self.store.load(review_unit, path)?)
    }

    fn compare_record(
        &self,
        snapshot: &Snapshot,
        file: &ChangedFile,
        record: LoadResult,
    ) -> eyre::Result<ReviewComparison> {
        let review_unit = snapshot.identity.review_unit();
        let path = file.review_path().as_bytes();
        let record = match record {
            LoadResult::Unreviewed => return Ok(ReviewComparison::Unreviewed(None)),
            LoadResult::UnknownSchema => {
                return Ok(ReviewComparison::Unreviewed(Some(
                    ReviewWarning::UnknownSchema,
                )));
            }
            LoadResult::Reviewed(ReviewRecord {
                partial: Some(partial),
                baseline_commit_id,
                ..
            }) => return self.compare_partial(snapshot, file, &partial, &baseline_commit_id),
            LoadResult::Reviewed(record) => record,
        };

        match self
            .repository
            .interdiff(&record.baseline_commit_id, snapshot, file.review_path())?
        {
            Interdiff::MissingBaseline => {
                self.store
                    .unreview_expired(review_unit, path, &record.reviewed_at)?;
                Ok(ReviewComparison::Unreviewed(Some(
                    ReviewWarning::BaselineExpired,
                )))
            }
            Interdiff::Diff(diff) => Ok(ReviewComparison::Compared {
                baseline_commit_id: record.baseline_commit_id,
                diff,
            }),
        }
    }

    pub fn unreview(&self, snapshot: &Snapshot, file: &ChangedFile) -> eyre::Result<()> {
        let _lock = self.store.lock_marks(snapshot.identity.review_unit())?;
        self.store.unreview(
            snapshot.identity.review_unit(),
            file.review_path().as_bytes(),
        )?;
        Ok(())
    }
}

pub(crate) struct Versions {
    base: Vec<u8>,
    reviewed: Reviewed,
    current: Vec<u8>,
}

impl Versions {
    pub(crate) fn review(&self) -> HunkReview<'_> {
        HunkReview::new(&self.base, &self.reviewed.text, &self.current)
            .attributed(&self.reviewed.attribution)
    }
}

fn whole_file_reviewed_version(diff: &ReviewDiff) -> Option<Vec<u8>> {
    reverse_apply(
        &diff.unified,
        diff.new_content.as_deref().unwrap_or_default(),
    )
}

#[cfg(test)]
#[path = "review.tests.rs"]
mod tests;
