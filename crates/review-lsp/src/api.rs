use std::ops::Range;
use std::path::Path;
use std::path::PathBuf;

use toasts::ToastId;

#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::FromRepr)]
#[repr(usize)]
pub enum Operation {
    Hover,
    Definition,
    References,
    TypeDefinition,
}

impl Operation {
    pub fn title(self) -> &'static str {
        match self {
            Self::Hover => "Documentation",
            Self::Definition => "Definitions",
            Self::TypeDefinition => "Type definitions",
            Self::References => "References",
        }
    }

    pub fn progress_text(self) -> &'static str {
        match self {
            Self::Hover => "Loading documentation…",
            Self::Definition => "Finding definition…",
            Self::TypeDefinition => "Finding type definition…",
            Self::References => "Finding references…",
        }
    }

    pub fn filter_locations(
        self,
        root: &Path,
        locations: Vec<SourceLocation>,
    ) -> Vec<SourceLocation> {
        locations
            .into_iter()
            .filter(|location| self != Self::References || location.path.starts_with(root))
            .collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Query {
    pub toast_id: ToastId,
    pub path: PathBuf,
    pub line: u32,
    pub byte_column: usize,
    pub expected_line: String,
    pub snapshot_id: String,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SourceLocation {
    pub path: PathBuf,
    pub line: u32,
    pub byte_column: usize,
    pub end_line: u32,
    pub end_byte_column: usize,
}

impl SourceLocation {
    pub fn review_path(&self, root: &Path) -> Option<String> {
        let path = if self.path.is_absolute() {
            self.path.strip_prefix(root).ok()?
        } else {
            &self.path
        };
        Some(path.display().to_string())
    }

    pub fn display_path(&self, root: &Path) -> String {
        self.review_path(root)
            .unwrap_or_else(|| self.path.display().to_string())
    }

    pub fn range_in_line(&self, line: u32, length: usize) -> Option<Range<usize>> {
        if line < self.line || line > self.end_line {
            return None;
        }
        Some(if self.line == self.end_line {
            self.byte_column.min(length)..self.end_byte_column.min(length)
        } else if line == self.line {
            self.byte_column.min(length)..length
        } else if line == self.end_line {
            0..self.end_byte_column.min(length)
        } else {
            0..length
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileChange {
    pub path: PathBuf,
    pub kind: FileChangeKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileChangeKind {
    Created,
    Changed,
    Deleted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Command {
    OpenDocument(PathBuf),
    Request { operation: Operation, query: Query },
    FilesChanged(Vec<FileChange>),
    Restart,
    Shutdown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServerStartup {
    pub id: ToastId,
    pub name: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Event {
    Initializing(ServerStartup),
    Ready(ServerStartup),
    Hover {
        toast_id: ToastId,
        snapshot_id: String,
        markdown: Option<String>,
    },
    Locations {
        toast_id: ToastId,
        operation: Operation,
        snapshot_id: String,
        locations: Vec<SourceLocation>,
        unindexed_files: usize,
    },
    Failed {
        toast_id: Option<ToastId>,
        snapshot_id: Option<String>,
        message: String,
    },
}

#[cfg(test)]
#[path = "api.tests.rs"]
mod tests;
