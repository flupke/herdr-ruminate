mod explore;
mod threads;
pub use explore::{
    EvidenceView, ExploreAnswerCancelled, ExploreCaptured, ExploreCommitted,
    ExploreComparisonAccepted, ExploreEvidence, ExploreEvidenceInput, ExploreFinished,
    ExploreHistoryChanged, ExploreImplementationFinished, ExploreImplementationSaved,
    ExplorePageNotOpened, ExplorePageNotShared, ExplorePageOffNetwork, ExplorePageReset,
    ExplorePageSettingsLoaded, ExplorePageShared, ExplorePageStart, ExplorePageStopped,
    ExplorePageTunnel, ExplorePositionsChanged, ExplorePositionsRestored, ExplorePosted,
    ExploreProgress, ExploreRestored, ExploreRoundSettingsLoaded, ExploreRunAheadUnavailable,
    ExploreStartBlock, ExploreStorageFailed, ExploreSubmission, ExploreTurnFinishing,
    ExploreTurnPath, ExploreViewports, ImplementationProgress,
};
pub use threads::{
    EventSettled, NewRepliesRequested, ReviewNavigation, ReviewNavigationChanged, ReviewPane,
    ReviewPaneFocusRequested, ReviewThreadsLoaded, TextPasted, ThreadFilesChanged,
    ThreadPostFinished, ThreadSelectionChanged, ThreadWakeupReported,
};

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Instant;

use review_lsp::{Operation, SourceLocation};
use review_repository::diff::DiffRow;
use review_repository::repository::ChangedFile;
use review_source::{DiffTarget, ReviewCheckpoint};
use review_state::{ReviewProgress, ReviewState, ReviewStatus};
use review_types::ReviewUnit;
use toasts::ToastKind;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositoryMetadataChanged {
    pub review_checkpoint: ReviewCheckpoint,
    pub description: String,
    pub display_id: String,
    pub issues: Vec<linear_issues::LinearIssue>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FilesOverviewChanged {
    pub progress: ReviewProgress,
    pub lines_added: u64,
    pub lines_removed: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SearchStatusChanged {
    pub query: Option<String>,
    pub current_match: usize,
    pub total_matches: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocationSelection {
    pub operation: Operation,
    pub locations: Vec<SourceLocation>,
    pub unindexed_files: usize,
    pub selected: usize,
    pub scroll: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PointerPosition {
    pub terminal_column: u16,
    pub terminal_row: u16,
    pub component_column: u16,
    pub component_row: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerInputKind {
    Scroll(isize),
    Click,
    ControlClick,
    DoubleClick,
    RightClick,
    Drag,
    Release,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PointerInput {
    pub kind: PointerInputKind,
    pub position: Option<PointerPosition>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommitMessageToggleRequested;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RevisionSelectorRequested;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ViewportChanged {
    pub width: u16,
    pub height: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiffViewportChanged {
    pub width: u16,
    pub height: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiffInputClearRequested;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceContentLoaded {
    pub snapshot_id: String,
    pub location: SourceLocation,
    pub content: Vec<u8>,
    pub mode: SourceLoadMode,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceContentLoadFailed {
    pub snapshot_id: String,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceLoadMode {
    Preview,
    External,
    ThreadPeek,
}

impl SourceLoadMode {
    pub fn is_external(self) -> bool {
        self == Self::External
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceSessionChanged {
    pub snapshot_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LspQueryContext {
    pub path: PathBuf,
    pub line: u32,
    pub byte_column: usize,
    pub expected_line: String,
    pub snapshot_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LspQueryRequested {
    pub operation: review_lsp::Operation,
    pub query: LspQueryContext,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextMenuRequested {
    pub column: u16,
    pub row: u16,
    pub query: Option<LspQueryContext>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceLocationPreviewRequested {
    pub location: SourceLocation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceLocationAccepted {
    pub location: SourceLocation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocationListVisibilityChanged {
    pub visible: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToastRequested {
    pub text: String,
    pub kind: ToastKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ToastExpirationTick {
    pub now: Instant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresentationLocation {
    Context { old_line: u32, new_line: u32 },
    NewLine(u32),
    OldLine(u32),
    SourceRow(usize),
    GapStart(u32),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReviewLocation {
    LoadedDocument {
        review_unit: ReviewUnit,
        path: String,
        cursor: usize,
        presentation_location: Option<PresentationLocation>,
        column: usize,
    },
    Source {
        review_unit: ReviewUnit,
        location: SourceLocation,
    },
    Revision {
        review_unit: ReviewUnit,
    },
}

impl ReviewLocation {
    pub fn review_unit(&self) -> &ReviewUnit {
        match self {
            Self::LoadedDocument { review_unit, .. }
            | Self::Source { review_unit, .. }
            | Self::Revision { review_unit } => review_unit,
        }
    }

    #[must_use]
    pub fn for_review_unit(&self, review_unit: impl Into<ReviewUnit>) -> Self {
        let review_unit = review_unit.into();
        match self {
            Self::LoadedDocument {
                path,
                cursor,
                presentation_location,
                column,
                ..
            } => Self::LoadedDocument {
                review_unit,
                path: path.clone(),
                cursor: *cursor,
                presentation_location: *presentation_location,
                column: *column,
            },
            Self::Source { location, .. } => Self::Source {
                review_unit,
                location: location.clone(),
            },
            Self::Revision { .. } => Self::Revision { review_unit },
        }
    }

    pub fn same_line(&self, other: &Self) -> bool {
        match self {
            Self::LoadedDocument { .. } => self.same_loaded_document_line(other),
            Self::Source { .. } => self.same_source_line(other),
            Self::Revision { review_unit } => matches!(
                other,
                Self::Revision {
                    review_unit: other_review_unit
                } if review_unit == other_review_unit
            ),
        }
    }

    fn same_loaded_document_line(&self, other: &Self) -> bool {
        let Self::LoadedDocument {
            review_unit,
            path,
            cursor,
            presentation_location,
            ..
        } = self
        else {
            unreachable!("loaded-document comparison starts from a loaded document");
        };
        let Self::LoadedDocument {
            review_unit: other_review_unit,
            path: other_path,
            cursor: other_cursor,
            presentation_location: other_presentation_location,
            ..
        } = other
        else {
            return false;
        };
        review_unit == other_review_unit
            && path == other_path
            && same_presentation_line(
                *presentation_location,
                *other_presentation_location,
                *cursor,
                *other_cursor,
            )
    }

    fn same_source_line(&self, other: &Self) -> bool {
        let Self::Source {
            review_unit,
            location,
        } = self
        else {
            unreachable!("source comparison starts from a source location");
        };
        let Self::Source {
            review_unit: other_review_unit,
            location: other_location,
        } = other
        else {
            return false;
        };
        review_unit == other_review_unit
            && location.path == other_location.path
            && location.line == other_location.line
    }
}

fn same_presentation_line(
    location: Option<PresentationLocation>,
    other_location: Option<PresentationLocation>,
    cursor: usize,
    other_cursor: usize,
) -> bool {
    match (location, other_location) {
        (Some(location), Some(other_location)) => location == other_location,
        (None, None) => cursor == other_cursor,
        (Some(_), None) | (None, Some(_)) => false,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrentReviewLocationChanged {
    pub location: Option<ReviewLocation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewLocationJumped {
    pub origin: ReviewLocation,
    pub target: ReviewLocation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewLocationRestoreRequested {
    pub location: ReviewLocation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RepositoryRefreshStarted;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RepositoryRefreshFinished;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RevisionCandidatesLoaded {
    pub direction: review_repository::repository::RevisionDirection,
    pub result: Result<Vec<review_repository::repository::RevisionCandidate>, String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RevisionHistoryLoadId(u64);

impl RevisionHistoryLoadId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RevisionHistoryLoaded {
    pub load_id: RevisionHistoryLoadId,
    pub result: Result<Vec<review_repository::repository::RevisionHistoryLine>, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RevisionProgressLoaded {
    pub load_id: RevisionHistoryLoadId,
    pub change_id: review_repository::repository::ChangeId,
    pub progress: Result<ReviewProgress, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RevisionEditFailed {
    pub message: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileSummary {
    pub file: ChangedFile,
    pub review_state: ReviewState,
    pub temporary: bool,
    pub disk_path: Option<PathBuf>,
}

impl FileSummary {
    pub fn new(path: impl Into<String>, status: ReviewStatus) -> Self {
        Self::from_changed(&ChangedFile::modified(path.into()), status)
    }

    pub fn from_changed(file: &ChangedFile, status: ReviewStatus) -> Self {
        let state = match status {
            ReviewStatus::Unreviewed => ReviewState::unreviewed(file.statistics, None),
            ReviewStatus::Reviewed => ReviewState::reviewed(),
            ReviewStatus::ChangedSinceReview => ReviewState::changed_since_review(file.statistics),
            ReviewStatus::PartiallyReviewed => {
                ReviewState::partially_reviewed(file.statistics, None)
            }
        };
        Self::from_review_state(file, state)
    }

    pub fn from_review_state(file: &ChangedFile, state: ReviewState) -> Self {
        Self {
            file: file.clone(),
            review_state: state,
            temporary: false,
            disk_path: None,
        }
    }

    pub fn temporary(
        path: impl Into<String>,
        display_path: impl Into<String>,
        disk_path: PathBuf,
    ) -> Self {
        let path = path.into();
        let mut file = ChangedFile::modified(&path);
        file.display_path = display_path.into();
        Self {
            file,
            review_state: ReviewState::reviewed(),
            temporary: true,
            disk_path: Some(disk_path),
        }
    }

    pub fn path(&self) -> String {
        self.file.review_path().display()
    }

    pub fn display_path(&self) -> &str {
        &self.file.display_path
    }
}

#[derive(Debug, Default)]
struct ReviewableFilePaths {
    initialized: bool,
    paths: HashSet<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ReviewableFiles(Arc<RwLock<ReviewableFilePaths>>);

impl ReviewableFiles {
    pub fn replace(&self, paths: HashSet<String>) -> bool {
        let mut current = self
            .0
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if current.initialized && current.paths == paths {
            return false;
        }
        current.initialized = true;
        current.paths = paths;
        true
    }

    pub fn contains(&self, path: &str) -> bool {
        let current = self
            .0
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        !current.initialized || current.paths.contains(path)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReviewableFilesChanged;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositoryFilesChanged {
    pub review_checkpoint: ReviewCheckpoint,
    pub files: Vec<FileSummary>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TemporaryFilesChanged {
    pub files: Vec<FileSummary>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewStateSaved {
    pub review_unit: ReviewUnit,
    pub path: String,
    pub result: Result<ReviewState, ()>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnreviewAllRequested(pub ReviewCheckpoint);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DisplayedDiffRow {
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DisplayedDiffViewport {
    pub path: String,
    pub file_index: usize,
    pub rows: Vec<DisplayedDiffRow>,
    pub can_show_file: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DisplayedDiffViewportsChanged {
    pub viewports: Vec<DisplayedDiffViewport>,
    pub current_file_index: usize,
    pub current_row: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffTargetJumpRequested {
    pub file_index: usize,
    pub row: Option<usize>,
    pub target: DiffTarget,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AnimationTick;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FileDecorationsChanged {
    pub notice_paths: Vec<String>,
    pub search_match_paths: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FilesViewportChanged {
    pub rows: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileSelectionRequested {
    pub path: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileSelected {
    pub path: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffContentLoaded {
    pub review_checkpoint: ReviewCheckpoint,
    pub path: String,
    pub rows: Vec<DiffRow>,
    pub old_content: Option<Vec<u8>>,
    pub new_content: Option<Vec<u8>>,
    pub hunks: review_hunks::FileHunks,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HighlightRequest {
    Diff(Arc<DiffContentLoaded>),
    Source(Arc<SourceContentLoaded>),
}

impl HighlightRequest {
    pub fn same_content(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Diff(left), Self::Diff(right)) => Arc::ptr_eq(left, right),
            (Self::Source(left), Self::Source(right)) => Arc::ptr_eq(left, right),
            _ => false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HighlightingFinished {
    pub request: HighlightRequest,
    pub highlighted: syntax_highlighting::HighlightedDiff,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffContentLoadFailed {
    pub review_checkpoint: ReviewCheckpoint,
    pub path: String,
}

#[derive(Clone, Copy, Debug)]
pub struct FrameRendered;
