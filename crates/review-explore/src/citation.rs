use std::borrow::Cow;
use std::path::Path;

use review_repository::diff::{DiffRow, NoticeKind, parse_file_diff};
use review_repository::repository::{RepoPath, Repository, Snapshot};
use review_source::SourceLineRange;

use crate::{CodeLocation, Comparison, Source, SourceSide};

pub struct CitedSource<'a> {
    pub source: Cow<'a, Source>,
    pub content: String,
    pub file: Option<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Uncitable {
    Path,
    Unreadable(String),
    Range,
    WholeFile,
    NoTextDiff(NoticeKind),
    Untracked,
    Gone,
}

impl std::fmt::Display for Uncitable {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Path => output.write_str("The cited path is outside the repository."),
            Self::Unreadable(error) => write!(
                output,
                "File-level evidence · non-text or unavailable source: {error}"
            ),
            Self::Range => {
                output.write_str("The saved evidence range is unavailable in this source.")
            }
            Self::WholeFile => output.write_str(
                "File-level evidence · the citation names the whole file, not lines of it, \
                 so no lines show here.",
            ),
            Self::NoTextDiff(kind) => output.write_str(no_text_diff(*kind)),
            Self::Untracked => output.write_str(
                "This file is not part of the repository's tracked files, so its lines do not \
                 show here.",
            ),
            Self::Gone => output.write_str(
                "The code this round cited is no longer available: the repository no longer \
                 holds the commit the round reviewed, and the round's saved diff does not hold \
                 these lines.",
            ),
        }
    }
}

fn check_range(range: &SourceLineRange, content: &str) -> Result<(), Uncitable> {
    if range.first_line == 0
        || range.last_line < range.first_line
        || range.last_line as usize > content.lines().count()
    {
        return Err(Uncitable::Range);
    }
    Ok(())
}

fn no_text_diff(kind: NoticeKind) -> &'static str {
    match kind {
        NoticeKind::Binary => "File-level evidence · the change of this file is binary.",
        NoticeKind::Conflict => {
            "File-level evidence · this file has an unresolved conflict, so the change shows no \
             lines of it."
        }
        NoticeKind::Unsupported => {
            "File-level evidence · the change of this file has no text diff."
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CitedLines {
    pub path: String,
    pub rows: Vec<DiffRow>,
    pub old_content: Option<Vec<u8>>,
    pub new_content: Option<Vec<u8>>,
}

impl CitedLines {
    pub fn in_diff(
        location: &CodeLocation,
        diff: &[DiffRow],
        old_content: Option<Vec<u8>>,
        new_content: Option<Vec<u8>>,
    ) -> Result<Self, Uncitable> {
        let range = location.lines.as_ref().ok_or(Uncitable::WholeFile)?;
        if let Some(DiffRow::Notice { kind, .. }) = diff
            .iter()
            .find(|row| matches!(row, DiffRow::Notice { .. }))
        {
            return Err(Uncitable::NoTextDiff(*kind));
        }
        let content = location
            .side
            .pick(old_content.as_deref(), new_content.as_deref())
            .and_then(|content| std::str::from_utf8(content).ok())
            .ok_or(Uncitable::Range)?;
        let rows = FileRows::new(diff, location.side, content).cited(range)?;
        Ok(Self {
            path: location.path.display(),
            rows,
            old_content,
            new_content,
        })
    }

    fn in_diff_alone(
        path: String,
        side: SourceSide,
        range: &SourceLineRange,
        diff: &[DiffRow],
    ) -> Result<Self, Uncitable> {
        if let Some(DiffRow::Notice { kind, .. }) = diff
            .iter()
            .find(|row| matches!(row, DiffRow::Notice { .. }))
        {
            return Err(Uncitable::NoTextDiff(*kind));
        }
        let file = FileRows {
            side,
            rows: diff
                .iter()
                .filter(|row| {
                    matches!(
                        row,
                        DiffRow::Context { .. } | DiffRow::Add { .. } | DiffRow::Delete { .. }
                    )
                })
                .cloned()
                .collect(),
        };
        let rows = file.cited(range).map_err(|_| Uncitable::Gone)?;
        let lines = rows.iter().filter(|row| file.line(row).is_some()).count();
        if lines != (range.last_line - range.first_line + 1) as usize {
            return Err(Uncitable::Gone);
        }
        Ok(Self {
            path,
            rows,
            old_content: None,
            new_content: None,
        })
    }

    fn unchanged(path: String, range: &SourceLineRange, content: String) -> Self {
        let lines: Vec<&str> = content.lines().collect();
        let rows = (range.first_line..=range.last_line)
            .map(|line| DiffRow::Context {
                old_line: line,
                new_line: line,
                text: format!(" {}", lines[line as usize - 1]),
            })
            .collect();
        let content = content.into_bytes();
        Self {
            path,
            rows,
            old_content: Some(content.clone()),
            new_content: Some(content),
        }
    }
}

struct FileRows {
    side: SourceSide,
    rows: Vec<DiffRow>,
}

impl FileRows {
    fn new(diff: &[DiffRow], side: SourceSide, content: &str) -> Self {
        let lines: Vec<&str> = content.lines().collect();
        let mut file = Self {
            side,
            rows: Vec::new(),
        };
        let (mut old, mut new) = (1, 1);
        for row in diff {
            let unchanged_before = match row {
                DiffRow::Context { new_line, .. } | DiffRow::Add { new_line, .. } => {
                    new_line.saturating_sub(new)
                }
                DiffRow::Delete { old_line, .. } => old_line.saturating_sub(old),
                _ => continue,
            };
            file.unchanged(&lines, &mut old, &mut new, unchanged_before);
            match row {
                DiffRow::Context {
                    old_line, new_line, ..
                } => (old, new) = (old_line + 1, new_line + 1),
                DiffRow::Add { new_line, .. } => new = new_line + 1,
                DiffRow::Delete { old_line, .. } => old = old_line + 1,
                _ => {}
            }
            file.rows.push(row.clone());
        }
        let rest = u32::try_from(lines.len())
            .unwrap_or(u32::MAX)
            .saturating_sub(side.pick(old, new) - 1);
        file.unchanged(&lines, &mut old, &mut new, rest);
        file
    }

    fn unchanged(&mut self, lines: &[&str], old: &mut u32, new: &mut u32, count: u32) {
        for _ in 0..count {
            let line = self.side.pick(*old, *new);
            let text = lines.get(line as usize - 1).copied().unwrap_or_default();
            self.rows.push(DiffRow::Context {
                old_line: *old,
                new_line: *new,
                text: format!(" {text}"),
            });
            *old += 1;
            *new += 1;
        }
    }

    fn line(&self, row: &DiffRow) -> Option<u32> {
        match (self.side, row) {
            (
                SourceSide::Old,
                DiffRow::Context { old_line: line, .. } | DiffRow::Delete { old_line: line, .. },
            )
            | (
                SourceSide::New,
                DiffRow::Context { new_line: line, .. } | DiffRow::Add { new_line: line, .. },
            ) => Some(*line),
            _ => None,
        }
    }

    fn cited(&self, range: &SourceLineRange) -> Result<Vec<DiffRow>, Uncitable> {
        let first = self
            .rows
            .iter()
            .position(|row| self.line(row) == Some(range.first_line));
        let last = self
            .rows
            .iter()
            .rposition(|row| self.line(row) == Some(range.last_line));
        match (first, last) {
            (Some(first), Some(last)) if first <= last => Ok(self.rows[first..=last].to_vec()),
            _ => Err(Uncitable::Range),
        }
    }
}

impl Comparison {
    pub fn cited_source(
        &self,
        location: &CodeLocation,
        root: &Path,
    ) -> Result<CitedSource<'_>, Uncitable> {
        let source = self.source(location).ok_or(Uncitable::Path)?;
        let content = source
            .read_text(root)
            .map_err(|error| Uncitable::Unreadable(error.to_string()))?;
        if let Some(range) = &location.lines {
            check_range(range, &content)?;
        }
        let file = self
            .files
            .iter()
            .position(|file| source.side.path_in(file) == Some(&source.path));
        Ok(CitedSource {
            source,
            content,
            file,
        })
    }

    pub fn tracked_cited_lines(
        &self,
        location: &CodeLocation,
        root: &Path,
    ) -> Result<CitedLines, Uncitable> {
        if !self.tracks(location, root) {
            return Err(Uncitable::Untracked);
        }
        self.cited_lines(location, root)
    }

    fn tracks(&self, location: &CodeLocation, root: &Path) -> bool {
        let changed = self
            .files
            .iter()
            .any(|file| location.side.path_in(file) == Some(&location.path));
        let base = CodeLocation {
            path: location.path.clone(),
            side: SourceSide::Old,
            lines: None,
        };
        changed
            || self
                .source(&base)
                .is_some_and(|source| source.read(root).is_ok())
    }

    pub fn cited_lines(
        &self,
        location: &CodeLocation,
        root: &Path,
    ) -> Result<CitedLines, Uncitable> {
        let range = location.lines.as_ref().ok_or(Uncitable::WholeFile)?;
        let cited = self.cited_source(location, root)?;
        let path = cited.source.display_path.clone();
        self.lines_of(
            location,
            range,
            path,
            cited.content,
            cited.file,
            |side, path| {
                self.source(&CodeLocation {
                    path: path.clone(),
                    side,
                    lines: None,
                })?
                .read_text(root)
                .ok()
                .map(String::into_bytes)
            },
        )
    }

    pub fn saved_cited_lines(
        &self,
        location: &CodeLocation,
        repository: &Repository,
    ) -> Result<CitedLines, Uncitable> {
        let range = location.lines.as_ref().ok_or(Uncitable::WholeFile)?;
        location.relative_path().ok_or(Uncitable::Path)?;
        let path = location.path.display();
        let index = self
            .files
            .iter()
            .position(|file| location.side.path_in(file) == Some(&location.path));
        let Some(content) = self.saved_file(repository, location.side, &location.path) else {
            let index = index.ok_or(Uncitable::Gone)?;
            return CitedLines::in_diff_alone(path, location.side, range, &self.diff_of(index));
        };
        let text = crate::source::text(content)
            .map_err(|error| Uncitable::Unreadable(error.to_string()))?;
        check_range(range, &text)?;
        self.lines_of(location, range, path, text, index, |side, path| {
            self.saved_file(repository, side, path)
        })
    }

    fn lines_of(
        &self,
        location: &CodeLocation,
        range: &SourceLineRange,
        path: String,
        content: String,
        index: Option<usize>,
        read: impl Fn(SourceSide, &RepoPath) -> Option<Vec<u8>>,
    ) -> Result<CitedLines, Uncitable> {
        let Some(index) = index else {
            return Ok(CitedLines::unchanged(path, range, content));
        };
        let file = &self.files[index];
        let content = content.into_bytes();
        let side = |side: SourceSide| {
            if side == location.side {
                return Some(content.clone());
            }
            read(side, side.path_in(file)?)
        };
        CitedLines::in_diff(
            location,
            &self.diff_of(index),
            side(SourceSide::Old),
            side(SourceSide::New),
        )
    }

    fn diff_of(&self, index: usize) -> Vec<DiffRow> {
        parse_file_diff(
            self.diffs.get(index).map_or(&[], Vec::as_slice),
            &self.files[index],
        )
    }

    fn saved_file(
        &self,
        repository: &Repository,
        side: SourceSide,
        path: &RepoPath,
    ) -> Option<Vec<u8>> {
        match side {
            SourceSide::New => repository
                .file_at(&self.checkpoint.checkpoint, path)
                .ok()
                .flatten(),
            SourceSide::Old => {
                let snapshot = Snapshot {
                    identity: self.base.clone()?,
                    files: Vec::new(),
                };
                repository.base_file_at(&snapshot, path).ok()
            }
        }
    }
}

#[cfg(test)]
#[path = "citation.tests.rs"]
mod tests;
