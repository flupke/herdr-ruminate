use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use review_explore::ExploreHistory;
use review_types::ReviewUnit;
use serde::Deserialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::explore::MAX_VIEW;
use super::{Error, Result, ReviewStore, StateKey};

const MERGED_WORKSPACES: &str = "merged-workspaces";
const MERGED_INTO: &str = "merged-into";
const FOREIGN_WORKSPACES: &str = "foreign-workspaces.json";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceState {
    key: String,
    path: PathBuf,
}

#[derive(Deserialize)]
struct MarkTime {
    reviewed_at: String,
}

#[derive(Deserialize)]
struct StoredThreadsOwner {
    conversations: StoredOwner,
}

#[derive(Deserialize)]
struct StoredOwner {
    review_unit: ReviewUnit,
}

#[derive(Deserialize)]
struct StoredRoundOwner {
    value: RoundOwner,
}

#[derive(Deserialize)]
struct RoundOwner {
    exploration: ExplorationOwner,
}

#[derive(Deserialize)]
struct ExplorationOwner {
    comparison: ComparisonOwner,
}

#[derive(Deserialize)]
struct ComparisonOwner {
    checkpoint: StoredOwner,
}

impl WorkspaceState {
    pub fn list(state_root: &Path) -> Result<Vec<Self>> {
        let entries = match fs::read_dir(state_root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => return Err(io("scan review state", state_root)(source)),
        };
        let mut states = Vec::new();
        for entry in entries {
            let entry = entry.map_err(io("scan review state", state_root))?;
            let Some(key) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if Self::is_key(&key) && entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                states.push(Self {
                    key,
                    path: entry.path(),
                });
            }
        }
        states.sort_by(|left, right| left.key.cmp(&right.key));
        Ok(states)
    }

    pub fn key_of(root: &Path) -> Result<String> {
        let root = root.to_str().ok_or(Error::InvalidStateKey {
            field: "workspace root",
        })?;
        Ok(StateKey::hash(root.as_bytes()).0)
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn is_key(name: &str) -> bool {
        name.len() == 64
            && name
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    }

    pub fn review_units(&self) -> Result<BTreeSet<ReviewUnit>> {
        let mut units = BTreeSet::new();
        for (name, _) in entries(&self.path.join("changes"))? {
            if name
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
            {
                units.insert(ReviewUnit::from(name));
            }
        }
        for kind in ["conversations", "drafts"] {
            for (_, path) in entries(&self.path.join(kind))? {
                if let Ok(unit) = record_unit(kind, &path) {
                    units.insert(unit);
                }
            }
        }
        for (_, review) in entries(&self.path.join("explore-v1"))? {
            if let Some(unit) = Self::explore_unit(&review) {
                units.insert(unit);
            }
        }
        units.retain(|unit| !unit.is_empty());
        Ok(units)
    }

    fn explore_unit(review: &Path) -> Option<ReviewUnit> {
        ReviewStore::round_files_in(review)
            .unwrap_or_default()
            .into_iter()
            .find_map(|(instance, _)| {
                let bytes = fs::read(review.join(format!("{instance}.json"))).ok()?;
                let round = serde_json::from_slice::<StoredRoundOwner>(&bytes).ok()?;
                Some(round.value.exploration.comparison.checkpoint.review_unit)
            })
    }

    pub fn last_modified(&self) -> Result<SystemTime> {
        latest_modification(&self.path)
    }
}

impl ReviewStore {
    pub fn directory(&self) -> &Path {
        &self.repository_dir
    }

    pub fn state_root(&self) -> &Path {
        &self.state_root
    }

    pub fn lock_migration(&self) -> Result<fs::File> {
        self.exclusive_lock(&self.state_root.join("migration.lock"), "lock migration")
    }

    pub fn foreign_workspace_states(&self) -> Result<BTreeSet<String>> {
        Ok(Self::read_json(
            &self.repository_dir.join(FOREIGN_WORKSPACES),
            "read foreign workspace states",
        )?
        .unwrap_or_default())
    }

    pub fn save_foreign_workspace_states(&self, keys: &BTreeSet<String>) -> Result<()> {
        self.atomic_json(
            &self.repository_dir.join(FOREIGN_WORKSPACES),
            keys,
            "write foreign workspace states",
        )
    }

    pub fn merge_workspace_state(&self, state: &WorkspaceState) -> Result<Vec<PathBuf>> {
        let old = Self {
            state_root: self.state_root.clone(),
            repository_dir: state.path.clone(),
            thread_sources: std::sync::Arc::default(),
        };
        let mut unreadable = Vec::new();
        self.merge_thread_contexts(&old)?;
        self.merge_marks(&old, &mut unreadable)?;
        self.merge_threads(&old, &mut unreadable)?;
        self.merge_drafts(&old, &mut unreadable)?;
        self.merge_explore(&old, &mut unreadable)?;
        Ok(unreadable)
    }

    pub fn retire_workspace_state(&self, state: &WorkspaceState) -> Result<PathBuf> {
        let merged = self.state_root.join(MERGED_WORKSPACES);
        self.create_dir(&merged)?;
        let mut target = merged.join(&state.key);
        let mut attempt = 1;
        while target.exists() {
            attempt += 1;
            target = merged.join(format!("{}-{attempt}", state.key));
        }
        fs::rename(&state.path, &target).map_err(io("move merged workspace state", &target))?;
        self.sync_parent(&target)?;
        let shared = self
            .repository_dir
            .strip_prefix(&self.state_root)
            .unwrap_or(&self.repository_dir);
        let note = format!("{}\n", shared.display());
        self.atomic_write(
            &target.join(MERGED_INTO),
            note.as_bytes(),
            "name merged state",
        )?;
        Ok(target)
    }

    fn merge_thread_contexts(&self, old: &Self) -> Result<()> {
        for (name, path) in entries(&old.repository_dir.join("thread-contexts"))? {
            let target = self.repository_dir.join("thread-contexts").join(&name);
            if !name.starts_with(".tmp-") && !target.exists() {
                let bytes = fs::read(&path).map_err(io("read thread context", &path))?;
                self.atomic_write(&target, &bytes, "copy thread context")?;
            }
        }
        Ok(())
    }

    fn merge_marks(&self, old: &Self, unreadable: &mut Vec<PathBuf>) -> Result<()> {
        for (unit, _) in entries(&old.repository_dir.join("changes"))? {
            let unit = ReviewUnit::from(unit);
            if super::checkpoint::ReviewUnitKey::validate(&unit).is_err() {
                unreadable.push(old.record_directory(&unit));
                continue;
            }
            let _lock = self.lock_marks(&unit)?;
            let mut changed = false;
            for (name, path) in entries(&old.record_directory(&unit))? {
                if name.starts_with(".tmp-") {
                    continue;
                }
                let bytes = fs::read(&path).map_err(io("read review mark", &path))?;
                let Ok(mark) = serde_json::from_slice::<MarkTime>(&bytes) else {
                    unreadable.push(path);
                    continue;
                };
                let target = self.record_directory(&unit).join(&name);
                let kept = fs::read(&target)
                    .ok()
                    .and_then(|kept| serde_json::from_slice::<MarkTime>(&kept).ok());
                if kept.is_none_or(|kept| later(&mark.reviewed_at, &kept.reviewed_at)) {
                    self.atomic_write(&target, &bytes, "merge review mark")?;
                    changed = true;
                }
            }
            if changed {
                self.stamp_marks(&unit)?;
            }
        }
        Ok(())
    }

    fn merge_threads(&self, old: &Self, unreadable: &mut Vec<PathBuf>) -> Result<()> {
        for (_, path) in entries(&old.repository_dir.join("conversations"))? {
            if path.extension().is_none_or(|extension| extension != "zst") {
                continue;
            }
            let document = record_unit("conversations", &path).and_then(|unit| {
                old.read_thread_document(&unit)
                    .map(|document| (unit, document))
            });
            let Ok((unit, document)) = document else {
                unreadable.push(path);
                continue;
            };
            self.update_threads(&unit, |threads| threads.merge(document.threads))?;
            if !document.embedded_drafts.is_empty() {
                self.update_drafts(&unit, |drafts, _| {
                    drafts.merge(document.embedded_drafts);
                    Ok(())
                })?;
            }
        }
        Ok(())
    }

    fn merge_drafts(&self, old: &Self, unreadable: &mut Vec<PathBuf>) -> Result<()> {
        for (_, path) in entries(&old.repository_dir.join("drafts"))? {
            if path.extension().is_none_or(|extension| extension != "zst") {
                continue;
            }
            let drafts = record_unit("drafts", &path)
                .and_then(|unit| old.read_drafts(&unit).map(|drafts| (unit, drafts)));
            let Ok((unit, drafts)) = drafts else {
                unreadable.push(path);
                continue;
            };
            self.update_drafts(&unit, |kept, _| {
                kept.merge(drafts);
                Ok(())
            })?;
        }
        Ok(())
    }

    fn merge_explore(&self, old: &Self, unreadable: &mut Vec<PathBuf>) -> Result<()> {
        for (name, from) in entries(&old.explore_directory())? {
            if !from.is_dir() {
                continue;
            }
            let to = self.explore_directory().join(&name);
            let _lock = self.lock_explore_review(&to)?;
            for (file, path) in entries(&from)? {
                let record = path
                    .extension()
                    .is_some_and(|extension| extension == "json");
                if record && file != "index.json" && !file.starts_with(".tmp-") {
                    self.copy_newer(&path, &to.join(&file))?;
                }
            }
            let index = from.join("index.json");
            let history = if let Ok(history) = Self::read_explore(&index, MAX_VIEW) {
                history
            } else {
                unreadable.push(index);
                Some(Self::rounds_in_saved_order(&to)?)
            };
            if let Some(history) = history {
                self.merge_history(&to, history)?;
            }
        }
        Ok(())
    }

    fn merge_history(&self, directory: &Path, old: ExploreHistory) -> Result<()> {
        let path = directory.join("index.json");
        let Ok(kept) = Self::read_explore::<ExploreHistory>(&path, MAX_VIEW) else {
            return Ok(());
        };
        let merged = match kept {
            None => old,
            Some(kept) if old.rounds.iter().all(|round| kept.rounds.contains(round)) => {
                return Ok(());
            }
            Some(_) => Self::rounds_in_saved_order(directory)?,
        };
        self.write_explore(&path, &merged, MAX_VIEW)
    }

    fn rounds_in_saved_order(directory: &Path) -> Result<ExploreHistory> {
        let mut rounds = Self::round_files_in(directory)?
            .into_iter()
            .map(|(instance, saved)| (saved, instance))
            .collect::<Vec<_>>();
        rounds.sort();
        Ok(ExploreHistory {
            rounds: rounds.into_iter().map(|(_, instance)| instance).collect(),
            latest_editable: false,
            ..ExploreHistory::default()
        })
    }

    fn copy_newer(&self, from: &Path, to: &Path) -> Result<()> {
        let saved = modified(from)?;
        if to.exists() && modified(to)? >= saved {
            return Ok(());
        }
        let bytes = fs::read(from).map_err(io("read Explore record", from))?;
        self.atomic_write(to, &bytes, "copy Explore record")?;
        fs::File::options()
            .write(true)
            .open(to)
            .and_then(|file| file.set_modified(saved))
            .map_err(io("keep Explore record time", to))
    }
}

fn record_unit(kind: &str, path: &Path) -> Result<ReviewUnit> {
    let bytes = fs::read(path).map_err(io("read review record", path))?;
    let json = ReviewStore::decode_thread_json(path, &bytes)?;
    let decode = Error::json("decode review record", path);
    Ok(if kind == "conversations" {
        serde_json::from_slice::<StoredThreadsOwner>(&json)
            .map_err(decode)?
            .conversations
            .review_unit
    } else {
        serde_json::from_slice::<StoredOwner>(&json)
            .map_err(decode)?
            .review_unit
    })
}

fn later(time: &str, than: &str) -> bool {
    match (
        OffsetDateTime::parse(time, &Rfc3339),
        OffsetDateTime::parse(than, &Rfc3339),
    ) {
        (Ok(time), Ok(than)) => time > than,
        _ => time > than,
    }
}

fn entries(directory: &Path) -> Result<Vec<(String, PathBuf)>> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(io("scan workspace state", directory)(source)),
    };
    let mut found = Vec::new();
    for entry in entries {
        let entry = entry.map_err(io("scan workspace state", directory))?;
        if let Some(name) = entry.file_name().to_str() {
            found.push((name.to_owned(), entry.path()));
        }
    }
    found.sort();
    Ok(found)
}

fn latest_modification(path: &Path) -> Result<SystemTime> {
    let mut latest = modified(path)?;
    if fs::symlink_metadata(path)
        .map_err(io("inspect workspace state", path))?
        .is_dir()
    {
        for (_, entry) in entries(path)? {
            latest = latest.max(latest_modification(&entry)?);
        }
    }
    Ok(latest)
}

fn modified(path: &Path) -> Result<SystemTime> {
    fs::symlink_metadata(path)
        .and_then(|metadata| metadata.modified())
        .map_err(io("inspect workspace state", path))
}

fn io(operation: &'static str, path: &Path) -> impl Fn(std::io::Error) -> Error {
    let path = path.to_owned();
    move |source| Error::StateIo {
        operation,
        path: path.clone(),
        source,
    }
}

#[cfg(test)]
#[path = "workspace_state.tests.rs"]
mod tests;
