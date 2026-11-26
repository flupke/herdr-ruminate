use review_explore::{CitedLines, CodeLocation, EvidenceRef, SourceSide, Uncitable};
use review_explore_citations::{Citation, CodeColors};
use review_repository::diff::parse_file_diff;
use review_repository::repository::{ChangeKind, ChangedFile, DiffStatistics, FileKind, RepoPath};
use review_source::SourceLineRange;

pub(crate) struct ChangedSource {
    pub(crate) path: &'static str,
    pub(crate) old: &'static str,
    pub(crate) new: &'static str,
    pub(crate) diff: &'static str,
}

impl ChangedSource {
    pub(crate) fn evidence(
        &self,
        side: SourceSide,
        lines: Option<(u32, u32)>,
        notes: &str,
    ) -> EvidenceRef {
        EvidenceRef {
            location: CodeLocation {
                path: RepoPath::from_bytes(self.path),
                side,
                lines: lines.map(|(first_line, last_line)| SourceLineRange {
                    first_line,
                    last_line,
                }),
            },
            notes: notes.into(),
        }
    }

    fn cited_lines(&self, location: &CodeLocation) -> Result<CitedLines, Uncitable> {
        CitedLines::in_diff(
            location,
            &parse_file_diff(self.diff.as_bytes(), &self.changed_file()),
            Some(self.old.into()),
            Some(self.new.into()),
        )
    }

    pub(crate) fn statistics(&self) -> DiffStatistics {
        self.changed_file().statistics
    }

    fn changed_file(&self) -> ChangedFile {
        let rows = || {
            self.diff
                .lines()
                .filter(|line| !line.starts_with("+++") && !line.starts_with("---"))
        };
        ChangedFile {
            old_path: Some(RepoPath::from_bytes(self.path)),
            new_path: Some(RepoPath::from_bytes(self.path)),
            old_kind: FileKind::File,
            new_kind: FileKind::File,
            change: ChangeKind::Modified,
            display_path: self.path.into(),
            statistics: DiffStatistics {
                lines_added: rows().filter(|line| line.starts_with('+')).count() as u64,
                lines_removed: rows().filter(|line| line.starts_with('-')).count() as u64,
            },
        }
    }
}

pub(crate) struct FixedChange(pub(crate) &'static [ChangedSource]);

impl FixedChange {
    pub(crate) fn cite(&self, evidence: &EvidenceRef) -> Citation {
        let path = &evidence.location.path;
        let lines = self
            .0
            .iter()
            .find(|source| *path == RepoPath::from_bytes(source.path))
            .map_or(Err(Uncitable::Untracked), |source| {
                source.cited_lines(&evidence.location)
            });
        CodeColors::default().cite(evidence.clone(), lines)
    }
}
