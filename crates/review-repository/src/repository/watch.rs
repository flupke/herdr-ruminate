use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WatchPlan {
    pub root: PathBuf,
    pub metadata: Vec<MetadataWatch>,
    pub git_excludes: Option<Vec<PathBuf>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataWatch {
    pub directory: PathBuf,
    pub scope: MetadataScope,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetadataScope {
    Entries,
    Subtree,
}

impl MetadataWatch {
    pub fn includes(&self, path: &Path) -> bool {
        match self.scope {
            MetadataScope::Entries => path.parent() == Some(self.directory.as_path()),
            MetadataScope::Subtree => path.starts_with(&self.directory),
        }
    }
}

impl WatchPlan {
    pub(super) fn new(root: &Path) -> Self {
        Self {
            root: root.to_owned(),
            metadata: Vec::new(),
            git_excludes: None,
        }
    }

    pub(super) fn add_metadata(&mut self, directory: &Path, scope: MetadataScope) {
        let Ok(directory) = fs::canonicalize(directory) else {
            return;
        };
        if !self
            .metadata
            .iter()
            .any(|target| target.directory == directory)
        {
            self.metadata.push(MetadataWatch { directory, scope });
        }
    }
}

// jj (`.jj/repo`) and Git (`commondir`) use files that hold a relative path for shared metadata.
pub(super) fn resolve_directory_or_link_file(path: &Path) -> Option<PathBuf> {
    if path.is_dir() {
        return fs::canonicalize(path).ok();
    }
    let value = fs::read_to_string(path).ok()?;
    fs::canonicalize(path.parent()?.join(value.trim())).ok()
}
