mod marks;
mod round;
mod tally;

pub use marks::ChangeMarks;
use marks::{FileMarks, Marks};
use review_explore::{ExploreRound, InterviewUpdate, MarkableFiles};
use review_hunks::LineCount;
use review_repository::repository::DiffStatistics;
use serde::Serialize;
use ts_rs::TS;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, TS)]
pub struct MarkTally {
    pub change: Tally,
    pub files: Vec<FileTally>,
    pub gain: Option<Gain>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, TS)]
pub struct FileTally {
    pub path: String,
    pub tally: Tally,
    pub whole: Option<WholeFile>,
    pub cited: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, TS)]
pub struct Tally {
    pub changed: DiffStatistics,
    pub marked: MarkedLines,
    pub pending: PendingLines,
    pub reopened: u64,
    pub left: u64,
    pub share: Share,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, TS)]
pub struct MarkedLines {
    pub answers: u64,
    pub jev: u64,
    pub not_relevant: u64,
    pub by_hand: u64,
    pub other_rounds: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, TS)]
pub struct PendingLines {
    pub reviewed: u64,
    pub not_relevant: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, TS)]
pub struct Share {
    pub marked: u64,
    pub changed: u64,
    pub percent: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, TS)]
pub struct Gain {
    pub pending: PendingLines,
    pub reopened: u64,
    pub before: Share,
    pub after: Share,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, TS)]
pub struct WholeFile {
    pub marked_by: Option<Marker>,
    pub answering: Option<WholeChange>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Marker {
    Answer,
    Jev,
    NotRelevant,
    ByHand,
    OtherRound,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum WholeChange {
    Reviewed,
    NotRelevant,
    Reopened,
}

impl MarkTally {
    pub(crate) fn of(
        files: &[FileMarks],
        round: Option<&ExploreRound>,
        question: Option<&InterviewUpdate>,
        markable: &MarkableFiles,
    ) -> Self {
        let round = round::RoundMarks::new(round);
        let answering = question.map(|question| round::Answering::new(question, markable));
        let files = files
            .iter()
            .map(|file| file.tally(&round, answering.as_ref()))
            .collect();
        Self::of_files(files, answering.is_some())
    }

    pub fn of_files(files: Vec<FileTally>, waits: bool) -> Self {
        let change = files.iter().map(|file| file.tally).sum();
        Self {
            gain: waits.then(|| Gain::of(&change)),
            change,
            files,
        }
    }
}

impl Gain {
    fn of(change: &Tally) -> Self {
        Self::new(change.pending, change.reopened, change.share)
    }

    pub fn new(pending: PendingLines, reopened: u64, before: Share) -> Self {
        let marked = (before.marked + pending.total()).saturating_sub(reopened);
        Self {
            pending,
            reopened,
            before,
            after: Share::of(marked.min(before.changed), before.changed),
        }
    }
}

impl MarkedLines {
    pub(crate) fn total(&self) -> u64 {
        self.answers + self.jev + self.not_relevant + self.by_hand + self.other_rounds
    }

    fn count(&mut self, marker: Marker) {
        *match marker {
            Marker::Answer => &mut self.answers,
            Marker::Jev => &mut self.jev,
            Marker::NotRelevant => &mut self.not_relevant,
            Marker::ByHand => &mut self.by_hand,
            Marker::OtherRound => &mut self.other_rounds,
        } += 1;
    }
}

impl PendingLines {
    pub(crate) fn total(&self) -> u64 {
        self.reviewed + self.not_relevant
    }
}

impl Share {
    pub fn of(marked: u64, changed: u64) -> Self {
        let lines = |count: u64| u32::try_from(count).unwrap_or(u32::MAX);
        let count = LineCount {
            reviewed: lines(marked),
            total: lines(changed),
        };
        Self {
            marked,
            changed,
            percent: u64::from(count.percent()),
        }
    }
}

fn count(lines: usize) -> u64 {
    u64::try_from(lines).unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "lib.tests.rs"]
mod tests;
