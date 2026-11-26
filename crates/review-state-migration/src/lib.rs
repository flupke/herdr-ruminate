mod backup;

use std::collections::BTreeSet;
use std::path::PathBuf;

use review_repository::repository::Repository;
use review_store::{ReviewStore, WorkspaceState};

#[derive(Debug, Default)]
pub struct Migration {
    pub merged: Vec<PathBuf>,
    pub unreadable: Vec<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Could not back up the review state; nothing was merged: {0}")]
    Backup(String),
    #[error("Could not merge the review state of this repository's other workspaces: {0}")]
    Merge(String),
}

pub fn migrate(repository: &Repository, store: &ReviewStore) -> Result<Migration, Error> {
    let merge = |error: &dyn std::fmt::Display| Error::Merge(error.to_string());
    let _lock = store.lock_migration().map_err(|error| merge(&error))?;
    let states = WorkspaceState::list(store.state_root()).map_err(|error| merge(&error))?;
    if states.is_empty() {
        return Ok(Migration::default());
    }
    let found = repository_states(repository, store, &states).map_err(|error| merge(&*error))?;
    if !found.ours.is_empty() {
        backup::before_merge(store, &states, &found.ours)
            .map_err(|error| Error::Backup(error.to_string()))?;
    }
    if let Some(foreign) = found.foreign {
        store
            .save_foreign_workspace_states(&foreign)
            .map_err(|error| merge(&error))?;
    }
    let mut migration = Migration::default();
    for state in found.ours {
        let unreadable = store
            .merge_workspace_state(&state)
            .map_err(|error| merge(&error))?;
        migration.unreadable.extend(unreadable);
        migration.merged.push(
            store
                .retire_workspace_state(&state)
                .map_err(|error| merge(&error))?,
        );
    }
    Ok(migration)
}

struct Found {
    ours: Vec<WorkspaceState>,
    foreign: Option<BTreeSet<String>>,
}

fn repository_states(
    repository: &Repository,
    store: &ReviewStore,
    states: &[WorkspaceState],
) -> Result<Found, Box<dyn std::error::Error>> {
    let current = repository
        .workspace_roots()?
        .iter()
        .map(|root| WorkspaceState::key_of(root))
        .collect::<Result<BTreeSet<_>, _>>()?;
    let known = store.foreign_workspace_states()?;
    let mut foreign = known.clone();
    let mut ours = Vec::new();
    for state in states {
        if current.contains(state.key()) {
            ours.push(state.clone());
        } else if !foreign.contains(state.key()) {
            let units = state.review_units()?;
            if units.is_empty() {
                continue;
            }
            if repository.holds_any_review_unit(&units)? {
                ours.push(state.clone());
            } else {
                foreign.insert(state.key().to_owned());
            }
        }
    }
    Ok(Found {
        ours,
        foreign: (foreign != known).then_some(foreign),
    })
}

#[cfg(test)]
#[path = "lib.tests.rs"]
mod tests;
