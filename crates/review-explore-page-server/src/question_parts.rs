use review_explore::{Alternative, CodeLocation, EvidenceRef, Question, SourceSide, TopicStatus};
use review_repository::repository::RepoPath;
use review_source::SourceLineRange;

pub(crate) fn question(
    id: &str,
    topic: &str,
    text: &str,
    alternatives: Vec<Alternative>,
    evidence: Vec<EvidenceRef>,
) -> Question {
    Question {
        id: id.into(),
        version: 1,
        topic: topic.into(),
        text: text.into(),
        rationale: None,
        visual: None,
        alternatives,
        evidence,
        assessments: None,
    }
}

pub(crate) fn alternative(id: &str, text: &str, outcome: TopicStatus) -> Alternative {
    Alternative {
        id: id.into(),
        text: text.into(),
        outcome,
        recommendation: None,
    }
}

pub(crate) fn lines(path: &str, first_line: u32, last_line: u32) -> CodeLocation {
    CodeLocation {
        path: RepoPath::from_bytes(path.as_bytes()),
        side: SourceSide::New,
        lines: Some(SourceLineRange {
            first_line,
            last_line,
        }),
    }
}
