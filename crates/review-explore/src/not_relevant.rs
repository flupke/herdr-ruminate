use review_repository::repository::RepoPath;
use review_source::SourceLineRange;
use serde::{Deserialize, Serialize};

use crate::{CodeLocation, Comparison, SourceSide};

/// Why changed lines hold no decision for the reviewer.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NotRelevantReason {
    /// Removed code whose removal is what the change is for.
    RemovedCode,
    /// Mechanics that are right or wrong rather than a choice, covered by
    /// the test named in `test`.
    TestedMechanics,
    /// Tests, docs and manifests that follow the code.
    FollowsCode,
}

/// Lines of a test, at the reviewed checkpoint.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TestLocation {
    /// Repository-relative UTF-8 path or lossless raw path bytes.
    #[serde(with = "crate::path_serde")]
    #[schemars(with = "crate::path_serde::PathInput")]
    pub path: RepoPath,
    /// One-based inclusive lines of the test.
    pub lines: SourceLineRange,
}

impl TestLocation {
    fn location(&self) -> CodeLocation {
        CodeLocation {
            path: self.path.clone(),
            side: SourceSide::New,
            lines: Some(self.lines.clone()),
        }
    }
}

impl std::fmt::Display for TestLocation {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(output, "{} {}", self.path.display(), self.lines)
    }
}

impl std::fmt::Display for NotRelevantMark {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(output, "{} ({})", self.location, self.why())
    }
}

/// Changed lines that hold no decision, and why.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq, schemars::JsonSchema)]
pub struct NotRelevantMark {
    #[serde(flatten)]
    pub location: CodeLocation,
    /// Why these lines hold no decision.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "NotRelevantReason", required)]
    pub reason: Option<NotRelevantReason>,
    /// The test that covers these lines, at the checkpoint: required for
    /// `tested_mechanics`, allowed for the other reasons.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test: Option<TestLocation>,
}

impl NotRelevantMark {
    pub fn locations(marks: &[Self]) -> impl Iterator<Item = &CodeLocation> {
        marks.iter().map(|mark| &mark.location)
    }

    fn why(&self) -> String {
        let Some(reason) = self.reason else {
            return "not relevant".into();
        };
        let reason = match reason {
            NotRelevantReason::RemovedCode => "removed code the change is about",
            NotRelevantReason::TestedMechanics => "mechanics covered by tests",
            NotRelevantReason::FollowsCode => "follows the code",
        };
        match &self.test {
            Some(test) => format!("not relevant: {reason}, see {test}"),
            None => format!("not relevant: {reason}"),
        }
    }

    pub(crate) fn validate(&self, comparison: &Comparison) -> eyre::Result<()> {
        let location = &self.location;
        eyre::ensure!(
            self.reason.is_some(),
            "not_relevant {location} needs a reason: removed_code, tested_mechanics or follows_code"
        );
        match &self.test {
            None => {
                eyre::ensure!(
                    self.reason != Some(NotRelevantReason::TestedMechanics),
                    "not_relevant {location} is tested_mechanics and needs `test`, the path and \
                     lines of a test that covers it"
                );
            }
            Some(test) => {
                eyre::ensure!(
                    comparison.validate_location(&test.location()),
                    "not_relevant {location}: its test {test} does not exist at the checkpoint"
                );
            }
        }
        Ok(())
    }
}
