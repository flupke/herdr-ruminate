use std::cell::RefCell;

use review_explore::{ExploreRound, MarkableFiles, TurnRequest};
use review_explore_tally::MarkTally;
use review_repository::repository::{Snapshot, SnapshotIdentity};

use crate::ExploreSession;

#[derive(Debug, Default)]
pub(crate) struct MarkableCache(RefCell<Option<MarkableAt>>);

#[derive(Debug)]
struct MarkableAt {
    instance: String,
    snapshot: SnapshotIdentity,
    files: MarkableFiles,
}

impl ExploreSession {
    pub fn mark_tally(&self) -> MarkTally {
        let round = self.state.round.as_ref();
        let question = round
            .filter(|_| !self.state.historical)
            .and_then(|round| round.exploration.waiting_turn());
        let markable = match (round, question, self.marks_snapshot.as_ref()) {
            (Some(round), Some(question), Some(snapshot)) => {
                self.turn_markable(round, &question.request, snapshot)
            }
            _ => MarkableFiles::All,
        };
        self.marks.mark_tally(round, question, &markable)
    }

    pub(crate) fn read_marks(&mut self, snapshot: &Snapshot) {
        self.marks.read(&self.tracker, snapshot);
        self.marks_snapshot = Some(snapshot.clone());
    }

    pub(crate) fn markable(&self, round: &ExploreRound, snapshot: &Snapshot) -> MarkableFiles {
        if round.is_at(&snapshot.identity) {
            return MarkableFiles::All;
        }
        let mut cache = self.markable.0.borrow_mut();
        let cached = cache.as_ref().filter(|cached| {
            cached.instance == round.exploration.instance && cached.snapshot == snapshot.identity
        });
        if let Some(cached) = cached {
            return cached.files.clone();
        }
        let files = MarkableFiles::at(&round.exploration.comparison, &self.repository, snapshot);
        if files.complete() {
            *cache = Some(MarkableAt {
                instance: round.exploration.instance.clone(),
                snapshot: snapshot.identity.clone(),
                files: files.clone(),
            });
        }
        files
    }

    pub(crate) fn turn_markable(
        &self,
        round: &ExploreRound,
        request: &str,
        snapshot: &Snapshot,
    ) -> MarkableFiles {
        let diverged = round
            .turns
            .get(request)
            .map_or(&[][..], |turn| turn.diverged.as_slice());
        self.markable(round, snapshot).excluding(snapshot, diverged)
    }

    pub(crate) fn diverged_at(&self, snapshot: &Snapshot) -> Vec<String> {
        let Some(round) = self.state.round.as_ref() else {
            return Vec::new();
        };
        self.markable(round, snapshot)
            .diverged(&round.exploration.comparison, snapshot)
    }

    pub(crate) fn diverged_now(&self) -> Vec<String> {
        (0..3)
            .find_map(|_| self.complete_snapshot().ok())
            .map(|snapshot| self.diverged_at(&snapshot))
            .unwrap_or_default()
    }

    pub(crate) fn record_diverged(&mut self, request: &TurnRequest) {
        let diverged = self
            .prompted_diverged
            .remove(&request.request)
            .unwrap_or_default();
        if diverged.is_empty() {
            return;
        }
        let checkpoint = &request.checkpoint;
        let saved = self
            .rounds
            .update(&checkpoint.review_unit, &request.instance, |round| {
                if let Some(turn) = round.turns.get_mut(&request.request) {
                    crate::records::merge_diverged(&mut turn.diverged, diverged.clone());
                }
                Ok(())
            });
        match saved {
            Ok(((), round)) => self.state.round = Some(round),
            Err(error) => {
                let _ = self
                    .events
                    .send(ui_events::ExploreStorageFailed(error.to_string()));
            }
        }
    }
}
