use std::collections::BTreeSet;

use review_repository::repository::{ChangedFile, RepoPath, Repository, Snapshot};

use crate::Comparison;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MarkableFiles {
    All,
    Unchanged {
        unchanged: BTreeSet<RepoPath>,
        unread: BTreeSet<RepoPath>,
    },
}

impl MarkableFiles {
    pub fn none() -> Self {
        Self::Unchanged {
            unchanged: BTreeSet::new(),
            unread: BTreeSet::new(),
        }
    }

    pub fn at(comparison: &Comparison, repository: &Repository, snapshot: &Snapshot) -> Self {
        let identity = &snapshot.identity;
        let checkpoint = &comparison.checkpoint;
        if checkpoint.matches(identity.review_unit(), identity.snapshot_id()) {
            return Self::All;
        }
        let mut unchanged = BTreeSet::new();
        let mut unread = BTreeSet::new();
        if identity.review_unit() != &checkpoint.review_unit {
            return Self::none();
        }
        for file in &snapshot.files {
            let Some(captured) = comparison.captured_diff(file) else {
                continue;
            };
            match repository.diff(snapshot, file) {
                Ok(current) if same_diff(&current, captured) => {
                    unchanged.insert(file.review_path().clone());
                }
                Ok(_) => {}
                Err(_) => {
                    unread.insert(file.review_path().clone());
                }
            }
        }
        Self::Unchanged { unchanged, unread }
    }

    pub fn allows(&self, file: &ChangedFile) -> bool {
        match self {
            Self::All => true,
            Self::Unchanged { unchanged, .. } => unchanged.contains(file.review_path()),
        }
    }

    pub fn unread(&self, file: &ChangedFile) -> bool {
        match self {
            Self::All => false,
            Self::Unchanged { unread, .. } => unread.contains(file.review_path()),
        }
    }

    #[must_use]
    pub fn excluding(self, snapshot: &Snapshot, diverged: &[String]) -> Self {
        if diverged.is_empty() {
            return self;
        }
        let unchanged = snapshot
            .files
            .iter()
            .filter(|file| self.allows(file))
            .filter(|file| !diverged.contains(&file.review_path().display()))
            .map(|file| file.review_path().clone())
            .collect();
        let unread = match self {
            Self::All => BTreeSet::new(),
            Self::Unchanged { unread, .. } => unread,
        };
        Self::Unchanged { unchanged, unread }
    }

    pub fn diverged(&self, comparison: &Comparison, snapshot: &Snapshot) -> Vec<String> {
        if *self == Self::All {
            return Vec::new();
        }
        comparison
            .files
            .iter()
            .filter(|captured| {
                match snapshot.files.iter().find(|file| {
                    file.old_path == captured.old_path && file.new_path == captured.new_path
                }) {
                    Some(file) => !self.allows(file) && !self.unread(file),
                    None => true,
                }
            })
            .map(|captured| captured.review_path().display())
            .collect()
    }

    pub fn complete(&self) -> bool {
        match self {
            Self::All => true,
            Self::Unchanged { unread, .. } => unread.is_empty(),
        }
    }
}

// Git abbreviates the object names of an `index` line to a length that grows with the
// repository.
fn same_diff(current: &[u8], captured: &[u8]) -> bool {
    let lines = |diff: &[u8]| {
        diff.split(|byte| *byte == b'\n')
            .map(<[u8]>::to_vec)
            .collect::<Vec<_>>()
    };
    let (current, captured) = (lines(current), lines(captured));
    current.len() == captured.len()
        && current
            .iter()
            .zip(&captured)
            .all(|(current, captured)| current == captured || same_index(current, captured))
}

fn same_index(current: &[u8], captured: &[u8]) -> bool {
    let parts = |line: &[u8]| -> Option<(Vec<u8>, Vec<u8>, Vec<u8>)> {
        let rest = line.strip_prefix(b"index ")?;
        let (objects, mode) = match rest.iter().position(|byte| *byte == b' ') {
            Some(space) => (&rest[..space], &rest[space..]),
            None => (rest, &[][..]),
        };
        let dots = objects.windows(2).position(|pair| pair == b"..")?;
        Some((
            objects[..dots].to_vec(),
            objects[dots + 2..].to_vec(),
            mode.to_vec(),
        ))
    };
    let prefixes = |a: &[u8], b: &[u8]| {
        !a.is_empty() && !b.is_empty() && (a.starts_with(b) || b.starts_with(a))
    };
    match (parts(current), parts(captured)) {
        (Some((old, new, mode)), Some((old_then, new_then, mode_then))) => {
            prefixes(&old, &old_then) && prefixes(&new, &new_then) && mode == mode_then
        }
        _ => false,
    }
}

impl Comparison {
    fn captured_diff(&self, file: &ChangedFile) -> Option<&[u8]> {
        let index = self.files.iter().position(|captured| {
            captured.old_path == file.old_path && captured.new_path == file.new_path
        })?;
        self.diffs.get(index).map(Vec::as_slice)
    }
}

#[cfg(test)]
#[path = "markable.tests.rs"]
mod tests;
