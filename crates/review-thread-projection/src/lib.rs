use std::cell::{Ref, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use component_core::{Component, ComponentSubscriptions};
use review_threads::{ReviewThread, ReviewThreads, ThreadCounts, ThreadId, ThreadPaths};
use review_types::ReviewUnit;
use ui_events::{FileSummary, RepositoryFilesChanged, ReviewThreadsLoaded};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThreadPlacement {
    Current,
    Hidden,
    Earlier,
    OutsideDiff,
    Original,
    Unavailable,
    ExploreRound,
}

impl ThreadPlacement {
    pub fn label(self) -> &'static str {
        match self {
            Self::Current => "Current code",
            Self::Hidden => "Hidden code",
            Self::Earlier => "Earlier code",
            Self::OutsideDiff => "Outside diff",
            Self::Original => "Saved context",
            Self::Unavailable => "Unavailable",
            Self::ExploreRound => "Explore round",
        }
    }
}

#[derive(Default)]
pub struct ThreadProjection {
    review_unit: Option<ReviewUnit>,
    files: HashSet<String>,
    paths: ThreadPaths,
    threads: Option<ReviewThreads>,
    previous: Option<ReviewThreads>,
    file_counts: HashMap<String, ThreadCounts>,
    placements: HashMap<ThreadId, ThreadPlacement>,
}

impl ThreadProjection {
    pub fn threads(&self) -> Option<&ReviewThreads> {
        self.threads.as_ref()
    }

    pub fn previous_threads(&self) -> Option<&ReviewThreads> {
        self.previous.as_ref()
    }

    pub fn current_path<'a>(&'a self, path: &'a str) -> &'a str {
        self.paths.resolve(path)
    }

    fn is_on_reviewed_file(&self, thread: &ReviewThread) -> bool {
        thread
            .path()
            .is_some_and(|path| self.files.contains(self.current_path(path)))
    }

    pub fn file_counts(&self, path: &str) -> ThreadCounts {
        self.file_counts.get(path).copied().unwrap_or_default()
    }

    pub fn placement(&self, thread: &ReviewThread) -> ThreadPlacement {
        if thread.round().is_some() {
            return ThreadPlacement::ExploreRound;
        }
        self.placements
            .get(&thread.id)
            .copied()
            .unwrap_or(if self.is_on_reviewed_file(thread) {
                ThreadPlacement::Original
            } else {
                ThreadPlacement::OutsideDiff
            })
    }

    fn list_files(&mut self, event: &RepositoryFilesChanged) {
        let review_unit = &event.review_checkpoint.review_unit;
        if self.review_unit.as_ref() != Some(review_unit) {
            *self = Self {
                review_unit: Some(review_unit.clone()),
                ..Self::default()
            };
        }
        self.files = event.files.iter().map(FileSummary::path).collect();
        self.paths = FileSummary::thread_paths(&event.files);
        self.count_files();
    }

    fn load_threads(&mut self, event: &ReviewThreadsLoaded) {
        if self.review_unit.as_ref() != Some(&event.review_unit) {
            return;
        }
        if let Ok(threads) = &event.result {
            self.previous = self.threads.replace(threads.clone());
            self.count_files();
        }
    }

    fn place(&mut self, review_unit: &ReviewUnit, placements: HashMap<ThreadId, ThreadPlacement>) {
        if self.review_unit.as_ref() == Some(review_unit) {
            self.placements = placements;
        }
    }

    fn count_files(&mut self) {
        self.file_counts = self.threads.as_ref().map_or_else(HashMap::new, |threads| {
            threads
                .counts_by(|thread| {
                    thread
                        .path()
                        .map(|path| self.paths.resolve(path).to_owned())
                })
                .into_iter()
                .filter_map(|(path, counts)| Some((path?, counts)))
                .collect()
        });
    }
}

#[derive(Clone, Default)]
pub struct SharedThreadProjection(Rc<RefCell<ThreadProjection>>);

impl SharedThreadProjection {
    /// # Panics
    ///
    /// Panics if called while the projection is being updated.
    pub fn read(&self) -> Ref<'_, ThreadProjection> {
        self.0.borrow()
    }

    /// # Panics
    ///
    /// Panics if called while the projection is borrowed.
    pub fn place(&self, review_unit: &ReviewUnit, placements: HashMap<ThreadId, ThreadPlacement>) {
        self.0.borrow_mut().place(review_unit, placements);
    }
}

pub struct ThreadProjector {
    projection: SharedThreadProjection,
}

impl ThreadProjector {
    pub fn new(projection: SharedThreadProjection) -> Self {
        Self { projection }
    }

    fn repository_changed(&mut self, event: &RepositoryFilesChanged) {
        self.projection.0.borrow_mut().list_files(event);
    }

    fn threads_loaded(&mut self, event: &ReviewThreadsLoaded) {
        self.projection.0.borrow_mut().load_threads(event);
    }
}

impl<A: Send + 'static> Component<A> for ThreadProjector {
    fn register_subscriptions(subscriptions: &mut ComponentSubscriptions<'_, Self, A>) {
        subscriptions.subscribe(Self::repository_changed);
        subscriptions.subscribe(Self::threads_loaded);
    }
}

#[cfg(test)]
#[path = "lib.tests.rs"]
mod tests;
