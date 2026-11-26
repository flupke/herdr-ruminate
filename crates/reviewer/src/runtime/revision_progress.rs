use std::collections::{HashMap, VecDeque};

use review_repository::repository::{ChangeId, Repository, RevisionHistoryLine, SnapshotId};
use review_state::{ReviewProgress, ReviewTracker};
use review_store::ReviewStore;
use ui_events::RevisionHistoryLoadId;

#[derive(Debug, Default)]
pub(super) struct HistoryProgress {
    load_id: Option<RevisionHistoryLoadId>,
    revisions: VecDeque<ChangeId>,
    queued: bool,
    known: HashMap<ChangeId, Known>,
}

#[derive(Debug)]
struct Known {
    commit: SnapshotId,
    stamp: Option<String>,
    progress: ReviewProgress,
}

pub(super) struct Started {
    pub(super) known: Vec<(ChangeId, ReviewProgress)>,
    pub(super) queue: bool,
}

impl HistoryProgress {
    pub(super) fn start(
        &mut self,
        load_id: RevisionHistoryLoadId,
        lines: &[RevisionHistoryLine],
        store: &ReviewStore,
        would_take_marks: &dyn Fn(&ChangeId, &SnapshotId) -> bool,
    ) -> Started {
        let current = lines.iter().position(|line| line.is_current).unwrap_or(0);
        let mut rows = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| !line.is_immutable && line.change_id.is_some())
            .collect::<Vec<_>>();
        rows.sort_by_key(|(row, _)| row.abs_diff(current));
        self.load_id = Some(load_id);
        self.revisions.clear();
        self.known.retain(|change_id, _| {
            rows.iter()
                .any(|(_, line)| line.change_id.as_ref() == Some(change_id))
        });
        let mut known = Vec::new();
        for (_, line) in rows {
            let Some(change_id) = line.change_id.clone() else {
                continue;
            };
            match self.still_known(&change_id, line.commit_id.as_ref(), store, would_take_marks) {
                Some(progress) => known.push((change_id, progress)),
                None => self.revisions.push_back(change_id),
            }
        }
        Started {
            known,
            queue: self.queue(),
        }
    }

    fn still_known(
        &self,
        change_id: &ChangeId,
        commit: Option<&SnapshotId>,
        store: &ReviewStore,
        would_take_marks: &dyn Fn(&ChangeId, &SnapshotId) -> bool,
    ) -> Option<ReviewProgress> {
        let known = self.known.get(change_id)?;
        let stamp = store.marks_stamp(change_id.review_unit()).ok()?;
        (Some(&known.commit) == commit
            && known.stamp == stamp
            && !would_take_marks(change_id, &known.commit))
        .then_some(known.progress)
    }

    pub(super) fn next(&mut self) -> Option<(RevisionHistoryLoadId, ChangeId)> {
        self.queued = false;
        Some((self.load_id?, self.revisions.pop_front()?))
    }

    pub(super) fn queue(&mut self) -> bool {
        let queue = !self.queued && !self.revisions.is_empty();
        self.queued |= queue;
        queue
    }

    pub(super) fn compute(
        &mut self,
        repository: &Repository,
        tracker: &ReviewTracker,
        store: &ReviewStore,
        change_id: &ChangeId,
    ) -> Result<ReviewProgress, String> {
        let stamp = store.marks_stamp(change_id.review_unit());
        let snapshot = match repository.snapshot_of(change_id) {
            Ok(Some(snapshot)) => snapshot,
            Ok(None) => return Err("the repository has no revisions to read".to_owned()),
            Err(error) => return Err(format!("could not read the revision: {error}")),
        };
        let states = tracker.statuses(&snapshot).map_err(|error| {
            format!("could not compare the revision with its review marks: {error}")
        })?;
        let progress = snapshot
            .files
            .iter()
            .zip(&states)
            .map(|(file, state)| (file.statistics, state))
            .collect();
        if let Ok(stamp) = stamp {
            self.known.insert(
                change_id.clone(),
                Known {
                    commit: SnapshotId::from(snapshot.identity.snapshot_id().to_owned()),
                    stamp,
                    progress,
                },
            );
        }
        Ok(progress)
    }
}

#[cfg(test)]
#[path = "revision_progress.tests.rs"]
mod tests;
