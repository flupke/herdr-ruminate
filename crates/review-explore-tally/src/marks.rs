use std::collections::HashMap;

use review_explore::{ExploreRound, InterviewUpdate, MarkableFiles};
use review_repository::repository::{ChangedFile, RepoPath, Snapshot, SnapshotIdentity};
use review_state::{FileLines, ReviewTracker};
use review_store::{LoadResult, ReviewStore};
use review_types::MarkAuthor;

use crate::MarkTally;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FileMarks {
    pub(crate) file: ChangedFile,
    pub(crate) marks: Marks,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Marks {
    Lines(FileLines),
    Whole(Option<MarkAuthor>),
    Unread,
}

#[derive(Debug)]
pub struct ChangeMarks {
    store: ReviewStore,
    snapshot: Option<SnapshotIdentity>,
    files: Vec<FileMarks>,
    records: HashMap<RepoPath, LoadResult>,
}

impl ChangeMarks {
    pub fn new(store: ReviewStore) -> Self {
        Self {
            store,
            snapshot: None,
            files: Vec::new(),
            records: HashMap::new(),
        }
    }

    pub fn read(&mut self, tracker: &ReviewTracker, snapshot: &Snapshot) {
        if self.snapshot.as_ref() != Some(&snapshot.identity) {
            self.snapshot = Some(snapshot.identity.clone());
            self.files.clear();
            self.records.clear();
        }
        let mut previous: HashMap<RepoPath, FileMarks> = std::mem::take(&mut self.files)
            .into_iter()
            .map(|file| (file.file.review_path().clone(), file))
            .collect();
        let records = std::mem::take(&mut self.records);
        let files = snapshot
            .files
            .iter()
            .map(|file| {
                self.read_file(
                    tracker,
                    snapshot,
                    file,
                    previous.remove(file.review_path()),
                    &records,
                )
            })
            .collect();
        self.files = files;
    }

    fn read_file(
        &mut self,
        tracker: &ReviewTracker,
        snapshot: &Snapshot,
        file: &ChangedFile,
        previous: Option<FileMarks>,
        records: &HashMap<RepoPath, LoadResult>,
    ) -> FileMarks {
        let path = file.review_path();
        let Ok(record) = self
            .store
            .load(snapshot.identity.review_unit(), path.as_bytes())
        else {
            return FileMarks {
                file: file.clone(),
                marks: Marks::Unread,
            };
        };
        let marks = previous
            .filter(|_| records.get(path) == Some(&record))
            .unwrap_or_else(|| FileMarks {
                file: file.clone(),
                marks: Marks::read(tracker, snapshot, file),
            });
        if marks.marks != Marks::Unread {
            self.records.insert(path.clone(), record);
        }
        marks
    }

    pub fn mark_tally(
        &self,
        round: Option<&ExploreRound>,
        question: Option<&InterviewUpdate>,
        markable: &MarkableFiles,
    ) -> MarkTally {
        MarkTally::of(&self.files, round, question, markable)
    }
}

impl Marks {
    fn read(tracker: &ReviewTracker, snapshot: &Snapshot, file: &ChangedFile) -> Self {
        match tracker.lines(snapshot, file) {
            Ok(lines) if lines != FileLines::default() => Self::Lines(lines),
            Ok(_) => tracker
                .whole_file_author(snapshot, file)
                .map_or(Self::Unread, Self::Whole),
            Err(_) => Self::Unread,
        }
    }
}
