//! The changes a commit was made from, by jj's record of the predecessors of each commit: the
//! change a `jj duplicate` copied, the one a `jj split` cut it from, those squashed into it.

use std::collections::{HashMap, HashSet, VecDeque};

use super::ChangeId;

/// The evolution of one commit, between changes: for its own change and each change met below
/// it, the changes met next on each line of predecessors below it. A line goes from a commit to
/// the commits jj records it was made from, and passes from one change to another where a
/// commit of the other is a predecessor. The evolution of a commit never changes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangeEvolution {
    change: ChangeId,
    below: HashMap<ChangeId, Vec<ChangeId>>,
}

/// What a change met on a line of an evolution is to the search for origins.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Candidate {
    /// An origin: the line ends at it.
    Origin,
    /// No origin, and the line ends at it.
    End,
    /// No origin: the line goes on below it.
    Past,
}

impl ChangeEvolution {
    /// The evolution of a commit of `change` that was made from no other change.
    pub fn of(change: ChangeId) -> Self {
        Self {
            change,
            below: HashMap::new(),
        }
    }

    /// The changes, other than its own, that `candidate` calls origins: on each line, the
    /// nearest change that is not one to go past, nearest first.
    pub fn origins(&self, mut candidate: impl FnMut(&ChangeId) -> Candidate) -> Vec<ChangeId> {
        let mut seen = HashSet::from([&self.change]);
        let mut queue = self.below(&self.change).iter().collect::<VecDeque<_>>();
        let mut origins = Vec::new();
        while let Some(change) = queue.pop_front() {
            if !seen.insert(change) {
                continue;
            }
            match candidate(change) {
                Candidate::Origin => origins.push(change.clone()),
                Candidate::End => {}
                Candidate::Past => queue.extend(self.below(change)),
            }
        }
        origins
    }

    fn below(&self, change: &ChangeId) -> &[ChangeId] {
        self.below.get(change).map_or(&[], Vec::as_slice)
    }

    /// Record that a line passes from `above` to `below`.
    pub(super) fn add(&mut self, above: &ChangeId, below: &ChangeId) {
        if above == below {
            return;
        }
        let next = self.below.entry(above.clone()).or_default();
        if !next.contains(below) {
            next.push(below.clone());
        }
    }

    /// Add the evolution of a predecessor commit, of `known.change`, which a line reached.
    pub(super) fn extend(&mut self, known: &Self) {
        for (above, below) in &known.below {
            for change in below {
                self.add(above, change);
            }
        }
    }
}

#[cfg(test)]
#[path = "evolution.tests.rs"]
mod tests;
