//! The review marks a change takes from the changes jj made it from: the change a
//! `jj duplicate` copied, the one a `jj split` cut it from, those squashed into it. A review
//! continues across such rewrites, though they give the code a new change ID.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use review_repository::repository::{Candidate, ChangeEvolution, ChangeId, Snapshot};
use review_types::ReviewUnit;

use super::ReviewTracker;

/// The evolutions of the commits read so far, by commit ID: an evolution never changes, so
/// each is walked once, and the walk of a later commit ends where it reaches one of them.
#[derive(Debug, Default)]
pub(super) struct Evolutions(Mutex<HashMap<String, Arc<ChangeEvolution>>>);

impl Evolutions {
    fn get(&self, commit: &str) -> Option<Arc<ChangeEvolution>> {
        self.lock().get(commit).cloned()
    }

    fn insert(&self, commit: &str, evolution: Arc<ChangeEvolution>) {
        self.lock().insert(commit.to_owned(), evolution);
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, Arc<ChangeEvolution>>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The searches that found origins but took no mark, by commit, with the stamps the origins'
/// marks had: the search is not run again until one of them changes.
#[derive(Debug, Default)]
pub(super) struct Fruitless(Mutex<HashMap<String, Vec<Option<String>>>>);

impl Fruitless {
    fn lock(&self) -> MutexGuard<'_, HashMap<String, Vec<Option<String>>>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl ReviewTracker {
    /// Give the change of `snapshot`, when it never had a review mark, the marks of its
    /// origins ([`ReviewStore::take_marks`](review_store::ReviewStore::take_marks) says which
    /// marks a path takes). The marks a change takes are a help on top of its own: a search that
    /// fails, such as while jj has two operation heads, leaves the change as it is, and the next
    /// read of it searches again.
    pub(super) fn take_origin_marks(&self, snapshot: &Snapshot) {
        let _ = self.search_origins(snapshot);
    }

    /// Whether the change of `snapshot` would take marks at its next read, as its origins'
    /// marks are now: false once it has marks of its own, or before its evolution was read.
    pub fn would_take_marks(&self, review_unit: &ReviewUnit, commit: &str) -> bool {
        self.store
            .marks_stamp(review_unit)
            .is_ok_and(|stamp| stamp.is_none())
            && self.store.has_marks(review_unit).is_ok_and(|marks| !marks)
            && self.evolutions.get(commit).is_some_and(|evolution| {
                let origins = self.origins(&evolution);
                !origins.is_empty() && !self.was_fruitless(commit, &origins)
            })
    }

    fn search_origins(&self, snapshot: &Snapshot) -> eyre::Result<()> {
        let review_unit = snapshot.identity.review_unit();
        if self.store.marks_stamp(review_unit)?.is_some() || self.store.has_marks(review_unit)? {
            return Ok(());
        }
        let commit = snapshot.identity.snapshot_id();
        let origins = self.origins(&*self.evolution(snapshot)?);
        if origins.is_empty() || self.was_fruitless(commit, &origins) {
            return Ok(());
        }
        // Read before the take: a mark made meanwhile changes them, and the next read searches
        // again.
        let stamps = self.stamps(&origins);
        let units = origins
            .iter()
            .map(|change| change.review_unit().clone())
            .collect::<Vec<_>>();
        let lock = self.store.lock_marks(review_unit)?;
        if !self.store.take_marks(&lock, review_unit, &units)? {
            self.fruitless.lock().insert(commit.to_owned(), stamps);
        }
        Ok(())
    }

    /// Whether a search of `commit` found `origins` with their marks as they are, and took none.
    fn was_fruitless(&self, commit: &str, origins: &[ChangeId]) -> bool {
        self.fruitless.lock().get(commit) == Some(&self.stamps(origins))
    }

    fn stamps(&self, origins: &[ChangeId]) -> Vec<Option<String>> {
        origins
            .iter()
            .map(|change| self.store.marks_stamp(change.review_unit()).ok().flatten())
            .collect()
    }

    fn evolution(&self, snapshot: &Snapshot) -> eyre::Result<Arc<ChangeEvolution>> {
        let commit = snapshot.identity.snapshot_id();
        if let Some(evolution) = self.evolutions.get(commit) {
            return Ok(evolution);
        }
        let evolution = Arc::new(
            self.repository
                .evolution(&snapshot.identity, &|commit| self.evolutions.get(commit))?,
        );
        self.evolutions.insert(commit, Arc::clone(&evolution));
        Ok(evolution)
    }

    /// On each line of `evolution`, the nearest change with marks, past the changes that never
    /// had any. A change whose marks were cleared, or cannot be read, ends its line: marks the
    /// reviewer reopened do not come back from further down.
    fn origins(&self, evolution: &ChangeEvolution) -> Vec<ChangeId> {
        evolution.origins(|change| {
            let review_unit = change.review_unit();
            match (
                self.store.has_marks(review_unit),
                self.store.marks_stamp(review_unit),
            ) {
                (Ok(true), _) => Candidate::Origin,
                (Ok(false), Ok(None)) => Candidate::Past,
                _ => Candidate::End,
            }
        })
    }
}

#[cfg(test)]
#[path = "carried_marks.tests.rs"]
mod tests;
