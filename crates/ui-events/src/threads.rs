use review_threads::{MessageId, ReviewThreads, SavedDrafts, ThreadId, WakeupNotice};
use review_types::ReviewUnit;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextPasted(pub String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewThreadsLoaded {
    pub review_unit: ReviewUnit,
    pub result: Result<ReviewThreads, String>,
    pub drafts: SavedDrafts,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreadPostFinished {
    pub review_unit: ReviewUnit,
    pub message_id: MessageId,
    pub result: Result<(), String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreadWakeupReported {
    pub review_unit: ReviewUnit,
    pub notice: Option<WakeupNotice>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ReviewNavigation {
    #[default]
    Files,
    Threads,
    Explore,
}

impl ReviewNavigation {
    #[allow(clippy::return_self_not_must_use)]
    pub fn next(self) -> Self {
        match self {
            Self::Files => Self::Threads,
            Self::Threads => Self::Explore,
            Self::Explore => Self::Files,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReviewNavigationChanged(pub ReviewNavigation);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EventSettled {
    pub mode: ReviewNavigation,
    pub focus: ReviewPane,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreadSelectionChanged {
    pub thread_id: Option<ThreadId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NewRepliesRequested;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewPane {
    Navigation,
    Detail,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReviewPaneFocusRequested(pub ReviewPane);

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ThreadFilesChanged {
    pub paths: Vec<String>,
}

impl super::FileSummary {
    pub fn thread_paths(files: &[Self]) -> review_threads::ThreadPaths {
        review_threads::ThreadPaths::new(files.iter().map(|file| {
            (
                file.path(),
                (file.file.change == review_repository::repository::ChangeKind::Renamed)
                    .then(|| {
                        file.file
                            .old_path
                            .as_ref()
                            .map(review_repository::repository::RepoPath::display)
                    })
                    .flatten(),
            )
        }))
    }
}
