use std::path::PathBuf;

use review_lsp::{Operation, Query, SourceLocation};
use review_repository::repository::{ChangeId, RevisionDirection};
use review_source::ReviewCheckpoint;

pub use ui_events::{RevisionHistoryLoadId, SourceLoadMode};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Action {
    Explore(review_explore::Command),
    ExplorePage(ExplorePageAction),
    Thread(review_threads::ThreadCommand),
    Document(DocumentAction),
    Lsp(LspAction),
    Settings(SettingsAction),
    Repository(RepositoryAction),
    Terminal(TerminalAction),
    OpenLink(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExplorePageAction {
    Open(review_explore_page_opening::PageScreen),
    OpenTunnel,
    CloseTunnel,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DocumentAction {
    Load(DocumentLoad),
    Highlight(ui_events::HighlightRequest),
    Search(Option<text_search::Request>),
    WatchSource(Option<PathBuf>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DocumentLoad {
    Diff {
        review_checkpoint: ReviewCheckpoint,
        path: String,
    },
    Diffs {
        review_checkpoint: ReviewCheckpoint,
        paths: Vec<String>,
    },
    Source {
        snapshot_id: String,
        location: SourceLocation,
        mode: SourceLoadMode,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LspAction {
    OpenDocument(PathBuf),
    Request { operation: Operation, query: Query },
    Restart,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SettingsAction {
    SaveFilePaneWidth(u16),
    SaveEditorKeymap(review_types::EditorKeymap),
    SaveExplorePage(review_explore_page_settings::ExplorePageSetting),
    SaveExploreWritingStyle(review_explore_round_settings::WritingStyle),
    SaveExploreRunAhead(review_explore_round_settings::RunAhead),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RepositoryAction {
    LoadRevisionCandidates(RevisionDirection),
    LoadRevisionHistory {
        load_id: RevisionHistoryLoadId,
    },
    EditRevision {
        change_id: ChangeId,
    },
    SetReviewed {
        path: String,
        reviewed: bool,
    },
    SetHunkReviewed {
        review_checkpoint: ReviewCheckpoint,
        path: String,
        mark: review_hunks::HunkMark,
    },
    AutoReview(ReviewCheckpoint),
    UnreviewAll(ReviewCheckpoint),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TerminalAction {
    OpenInEditor { path: PathBuf, line: Option<u32> },
    CopyToClipboard(String),
    Quit,
}
