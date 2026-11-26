use review_repository::diff::{DiffRow, parse_file_diff};
use review_repository::repository::{ChangedFile, Snapshot};

use crate::ReviewTracker;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RowChange {
    Unchanged,
    Removed,
    Added,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenRow {
    pub change: RowChange,
    pub base_line: Option<u32>,
    pub current_line: Option<u32>,
    pub text: String,
}

impl ReviewTracker {
    pub fn open_rows(
        &self,
        snapshot: &Snapshot,
        file: &ChangedFile,
    ) -> eyre::Result<Vec<Vec<OpenRow>>> {
        let diff = self.diff(snapshot, file)?;
        let versions = self.versions(snapshot, file, &diff)?;
        let base = versions.review().base_numbering();
        let base_line = |reviewed_line: u32| {
            base.base_line(reviewed_line.saturating_sub(1))
                .map(|line| line + 1)
        };
        let content = |text: String| text.get(1..).unwrap_or_default().to_owned();
        let mut hunks: Vec<Vec<OpenRow>> = Vec::new();
        for row in parse_file_diff(&diff.unified, file) {
            let row = match row {
                DiffRow::Hunk { .. } => {
                    hunks.push(Vec::new());
                    continue;
                }
                DiffRow::Context {
                    old_line,
                    new_line,
                    text,
                } => OpenRow {
                    change: RowChange::Unchanged,
                    base_line: base_line(old_line),
                    current_line: Some(new_line),
                    text: content(text),
                },
                DiffRow::Delete { old_line, text } => OpenRow {
                    change: RowChange::Removed,
                    base_line: base_line(old_line),
                    current_line: None,
                    text: content(text),
                },
                DiffRow::Add { new_line, text } => OpenRow {
                    change: RowChange::Added,
                    base_line: None,
                    current_line: Some(new_line),
                    text: content(text),
                },
                DiffRow::FileHeader { .. } | DiffRow::Meta { .. } | DiffRow::Notice { .. } => {
                    continue;
                }
            };
            if let Some(hunk) = hunks.last_mut() {
                hunk.push(row);
            }
        }
        Ok(hunks)
    }
}
