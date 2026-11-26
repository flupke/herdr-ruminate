use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{Error, Result, ReviewStore, StateKey};

pub(super) const REPOSITORIES: &str = "repositories";
const REPOSITORY_RECORD: &str = "repository.json";
const HASH_DIGITS: usize = 16;
const SLUG_BYTES: usize = 64;

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
struct RepositoryRecord {
    version: u8,
    kind: String,
    store: String,
}

pub(super) struct SharedStore {
    path: String,
}

impl SharedStore {
    pub(super) fn canonical(store: &Path) -> Result<Self> {
        let canonical = std::fs::canonicalize(store).map_err(|source| Error::StateIo {
            operation: "canonicalize repository store",
            path: store.to_owned(),
            source,
        })?;
        let path =
            canonical
                .into_os_string()
                .into_string()
                .map_err(|_| Error::InvalidStateKey {
                    field: "repository store",
                })?;
        Ok(Self { path })
    }

    pub(super) fn directory(&self, state_root: &Path) -> PathBuf {
        let hash = StateKey::hash(self.path.as_bytes()).0;
        state_root
            .join(REPOSITORIES)
            .join(format!("{}-{}", self.slug(), &hash[..HASH_DIGITS]))
    }

    fn kind(&self) -> &'static str {
        if Path::new(&self.path).ends_with(".jj/repo") {
            "jj"
        } else {
            "git"
        }
    }

    fn slug(&self) -> String {
        let store = Path::new(&self.path);
        let parent = if self.kind() == "jj" {
            store.parent().and_then(Path::parent)
        } else if store.file_name().is_some_and(|name| name == ".git") {
            store.parent()
        } else {
            Some(store)
        };
        let name = parent
            .and_then(Path::file_name)
            .map_or(b"repository".as_slice(), |name| name.as_encoded_bytes());
        name.iter()
            .take(SLUG_BYTES)
            .map(|&byte| {
                if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-') {
                    char::from(byte)
                } else {
                    '_'
                }
            })
            .collect()
    }

    fn record(&self) -> RepositoryRecord {
        RepositoryRecord {
            version: 1,
            kind: self.kind().to_owned(),
            store: self.path.clone(),
        }
    }
}

impl ReviewStore {
    pub(super) fn check_repository_record(&self, store: &SharedStore) -> Result<()> {
        let path = self.repository_dir.join(REPOSITORY_RECORD);
        match Self::read_json::<RepositoryRecord>(&path, "read repository record")? {
            Some(record) if record.store != store.path => Err(Error::StateCollision { path }),
            _ => Ok(()),
        }
    }

    pub(super) fn write_repository_record(&self, store: &SharedStore) -> Result<()> {
        let path = self.repository_dir.join(REPOSITORY_RECORD);
        if path.exists() {
            return Ok(());
        }
        self.atomic_json(&path, &store.record(), "write repository record")
    }
}

#[cfg(test)]
#[path = "layout.tests.rs"]
mod tests;
