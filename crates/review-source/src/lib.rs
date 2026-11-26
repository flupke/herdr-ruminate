use std::ops::Range;

use gix_imara_diff::{Algorithm, Diff, InternedInput};
use review_types::ReviewUnit;
use serde::{Deserialize, Serialize};

/// The repository identity and exact snapshot used for review.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub struct ReviewCheckpoint {
    pub review_unit: ReviewUnit,
    pub checkpoint: String,
}

impl ReviewCheckpoint {
    pub fn new(review_unit: impl Into<ReviewUnit>, checkpoint: impl Into<String>) -> Self {
        Self {
            review_unit: review_unit.into(),
            checkpoint: checkpoint.into(),
        }
    }

    pub fn matches(&self, review_unit: &ReviewUnit, checkpoint: &str) -> bool {
        &self.review_unit == review_unit && self.checkpoint == checkpoint
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum DiffTarget {
    Lines {
        path: String,
        old: Option<SourceLineRange>,
        new: Option<SourceLineRange>,
    },
    File {
        path: String,
    },
}

impl DiffTarget {
    pub fn path(&self) -> &str {
        match self {
            Self::Lines { path, .. } | Self::File { path } => path,
        }
    }
}

/// One inclusive, one-based line range in a source citation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, schemars::JsonSchema)]
pub struct SourceLineRange {
    pub first_line: u32,
    pub last_line: u32,
}

impl SourceLineRange {
    pub fn from_zero_based(lines: Range<u32>) -> Self {
        Self {
            first_line: lines.start + 1,
            last_line: lines.end,
        }
    }

    pub fn count(&self) -> u32 {
        (self.last_line + 1).saturating_sub(self.first_line)
    }
}

impl std::fmt::Display for SourceLineRange {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.first_line == self.last_line {
            write!(output, "{}", self.first_line)
        } else {
            write!(output, "{}-{}", self.first_line, self.last_line)
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FrozenFile {
    pub path: String,
    pub hunk_count: usize,
    pub old_path: Option<String>,
    pub new_path: Option<String>,
    pub old_content: Option<Vec<u8>>,
    pub new_content: Option<Vec<u8>>,
    pub hunks: Vec<FrozenHunk>,
    pub diff_hash: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FrozenHunk {
    pub old: Option<Range<u32>>,
    pub new: Option<Range<u32>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DiffRangeAnchor {
    pub source_checkpoint: String,
    pub old_path: Option<String>,
    pub new_path: Option<String>,
    pub old_lines: Option<Range<u32>>,
    pub new_lines: Option<Range<u32>>,
    #[serde(default)]
    pub target_kind: AnchorKind,
    pub source_hunk_count: usize,
    pub old_content: Option<Vec<u8>>,
    pub new_content: Option<Vec<u8>>,
    pub diff_hash: String,
}

impl DiffRangeAnchor {
    pub fn map_new_lines(&self, content: &[u8]) -> Option<Range<u32>> {
        map_side(
            self.new_lines.clone(),
            self.new_content.as_deref(),
            Some(content),
        )
        .ok()
        .flatten()
    }

    pub fn map_lines(
        &self,
        old_content: Option<&[u8]>,
        new_content: Option<&[u8]>,
    ) -> Option<FrozenHunk> {
        Some(FrozenHunk {
            old: map_side(
                self.old_lines.clone(),
                self.old_content.as_deref(),
                old_content,
            )
            .ok()?,
            new: map_side(
                self.new_lines.clone(),
                self.new_content.as_deref(),
                new_content,
            )
            .ok()?,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AnchorKind {
    #[default]
    Hunks,
    Lines,
}

fn map_side(
    interval: Option<Range<u32>>,
    source_content: Option<&[u8]>,
    current_content: Option<&[u8]>,
) -> Result<Option<Range<u32>>, ()> {
    match interval {
        None => Ok(None),
        Some(interval) => Ok(Some(
            transform_interval(
                interval,
                &histogram_edits(source_content.ok_or(())?, current_content.ok_or(())?),
            )
            .ok_or(())?,
        )),
    }
}

fn transform_interval(
    source: Range<u32>,
    edits: &[(Range<u32>, Range<u32>)],
) -> Option<Range<u32>> {
    let mut shift = 0_i64;
    for (before, after) in edits {
        if before.end <= source.start {
            shift += i64::from(after.end - after.start) - i64::from(before.end - before.start);
        } else if source.end <= before.start {
            break;
        } else {
            return None;
        }
    }
    let start = i64::from(source.start)
        .checked_add(shift)?
        .try_into()
        .ok()?;
    let end = i64::from(source.end).checked_add(shift)?.try_into().ok()?;
    Some(start..end)
}

fn histogram_edits(before: &[u8], after: &[u8]) -> Vec<(Range<u32>, Range<u32>)> {
    let input = InternedInput::new(before, after);
    Diff::compute(Algorithm::Histogram, &input)
        .hunks()
        .map(|hunk| (hunk.before, hunk.after))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_identity_requires_the_review_unit_and_snapshot() {
        let checkpoint = ReviewCheckpoint::new("review", "snapshot");
        assert!(checkpoint.matches(&ReviewUnit::from("review"), "snapshot"));
        assert!(!checkpoint.matches(&ReviewUnit::from("other"), "snapshot"));
    }

    #[test]
    fn mapping_retains_unchanged_lines_after_an_insertion() {
        let anchor = DiffRangeAnchor {
            source_checkpoint: "checkpoint".into(),
            old_path: None,
            new_path: Some("src/lib.rs".into()),
            old_lines: None,
            new_lines: Some(1..2),
            target_kind: AnchorKind::Lines,
            source_hunk_count: 1,
            old_content: None,
            new_content: Some(b"first\nsecond\n".to_vec()),
            diff_hash: String::new(),
        };
        assert_eq!(
            anchor.map_new_lines(b"inserted\nfirst\nsecond\n"),
            Some(2..3)
        );
        assert_eq!(anchor.map_new_lines(b"first\nchanged\n"), None);
    }
}
