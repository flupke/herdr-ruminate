use std::ops::Range;

use review_repository::diff::DiffRow;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct HunkSpan {
    pub old: Range<u32>,
    pub new: Range<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HunkMark {
    Review(HunkSpan),
    Unreview(HunkSpan),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenHunk {
    pub span: HunkSpan,
    pub since_review: bool,
    pub(crate) changed_lines: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewedHunk {
    pub span: HunkSpan,
    pub rows: Vec<DiffRow>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FileHunks {
    pub open: Vec<OpenHunk>,
    pub reviewed: Vec<ReviewedHunk>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LineCount {
    pub reviewed: u32,
    pub total: u32,
}

impl LineCount {
    pub fn percent(self) -> u32 {
        let percent = u64::from(self.reviewed) * 100 / u64::from(self.total.max(1));
        let percent = u32::try_from(percent).unwrap_or(100);
        if self.reviewed == 0 || self.reviewed >= self.total {
            percent
        } else {
            percent.clamp(1, 99)
        }
    }
}

impl FileHunks {
    pub fn unreviewed(rows: &[DiffRow]) -> Self {
        Self {
            open: open_hunks(rows),
            reviewed: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.open.is_empty() && self.reviewed.is_empty()
    }

    pub fn count(&self) -> Option<LineCount> {
        let reviewed = self
            .reviewed
            .iter()
            .flat_map(|hunk| &hunk.rows)
            .filter(|row| matches!(row, DiffRow::Add { .. } | DiffRow::Delete { .. }))
            .count();
        let reviewed = u32::try_from(reviewed).unwrap_or(u32::MAX);
        let open = self.open.iter().map(|hunk| hunk.changed_lines).sum::<u32>();
        (reviewed > 0).then_some(LineCount {
            reviewed,
            total: reviewed.saturating_add(open),
        })
    }
}

pub(crate) fn open_hunks(rows: &[DiffRow]) -> Vec<OpenHunk> {
    if rows.iter().any(|row| matches!(row, DiffRow::Notice { .. })) {
        return Vec::new();
    }
    let mut spans = Vec::new();
    let mut walk: Option<HunkWalk> = None;
    for row in rows {
        if let Some(started) = HunkWalk::start(row) {
            spans.extend(walk.replace(started).and_then(HunkWalk::finish));
        } else if let Some(walk) = walk.as_mut() {
            walk.push(row);
        }
    }
    spans.extend(walk.and_then(HunkWalk::finish));
    spans
}

#[derive(Clone, Copy)]
struct Position {
    old: u32,
    new: u32,
}

struct HunkWalk {
    at: Position,
    first: Option<Position>,
    last: Option<Position>,
    changed: u32,
}

impl HunkWalk {
    fn start(row: &DiffRow) -> Option<Self> {
        let DiffRow::Hunk {
            old_start,
            old_count,
            new_start,
            new_count,
        } = *row
        else {
            return None;
        };
        let first_line = |start: u32, count: u32| if count == 0 { start } else { start - 1 };
        Some(Self {
            at: Position {
                old: first_line(old_start, old_count),
                new: first_line(new_start, new_count),
            },
            first: None,
            last: None,
            changed: 0,
        })
    }

    fn push(&mut self, row: &DiffRow) {
        let (old, new) = match row {
            DiffRow::Context { .. } => (1, 1),
            DiffRow::Delete { .. } => (1, 0),
            DiffRow::Add { .. } => (0, 1),
            _ => return,
        };
        let changed = old + new == 1;
        if changed {
            self.first.get_or_insert(self.at);
            self.changed += 1;
        }
        self.at.old += old;
        self.at.new += new;
        if changed {
            self.last = Some(self.at);
        }
    }

    fn finish(self) -> Option<OpenHunk> {
        let (first, last) = (self.first?, self.last?);
        Some(OpenHunk {
            span: HunkSpan {
                old: first.old..last.old,
                new: first.new..last.new,
            },
            since_review: false,
            changed_lines: self.changed,
        })
    }
}

#[cfg(test)]
#[path = "hunks.tests.rs"]
mod tests;
