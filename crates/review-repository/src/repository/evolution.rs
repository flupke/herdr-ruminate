use std::collections::{HashMap, HashSet, VecDeque};

use super::ChangeId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangeEvolution {
    change: ChangeId,
    below: HashMap<ChangeId, Vec<ChangeId>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Candidate {
    Origin,
    End,
    Past,
}

impl ChangeEvolution {
    pub fn of(change: ChangeId) -> Self {
        Self {
            change,
            below: HashMap::new(),
        }
    }

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

    pub(super) fn add(&mut self, above: &ChangeId, below: &ChangeId) {
        if above == below {
            return;
        }
        let next = self.below.entry(above.clone()).or_default();
        if !next.contains(below) {
            next.push(below.clone());
        }
    }

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
