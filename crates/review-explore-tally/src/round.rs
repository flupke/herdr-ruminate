use review_explore::{
    ExploreRound, InterviewUpdate, MarkableFiles, NamedLines, NotRelevantMark, SourceSide,
    TurnMarks,
};
use review_hunks::{LineSelection, ReviewedLines};
use review_repository::repository::ChangedFile;
use review_types::MarkAuthor;

use crate::{Marker, PendingLines, WholeChange, count};

pub(crate) struct RoundMarks<'a>(Option<&'a ExploreRound>);

pub(crate) struct FileMarkers<'a> {
    round: &'a RoundMarks<'a>,
    not_relevant: Vec<(&'a str, NamedLines)>,
}

#[derive(Clone, Copy)]
pub(crate) enum Line {
    One(SourceSide, u32),
    Whole,
}

impl<'a> RoundMarks<'a> {
    pub(crate) fn new(round: Option<&'a ExploreRound>) -> Self {
        Self(round)
    }

    pub(crate) fn in_file(&'a self, file: &ChangedFile) -> FileMarkers<'a> {
        let marks = self
            .0
            .map(|round| round.marks.values())
            .into_iter()
            .flatten();
        FileMarkers {
            round: self,
            not_relevant: marks
                .filter_map(|marks: &TurnMarks| {
                    let answer = marks.answer.as_deref()?;
                    let named =
                        NamedLines::in_file(file, NotRelevantMark::locations(&marks.not_relevant))?;
                    Some((answer, named))
                })
                .collect(),
        }
    }

    fn has_answer(&self, answer: &str) -> bool {
        self.0.is_some_and(|round| {
            round
                .exploration
                .answers
                .iter()
                .any(|given| given.id == answer)
        })
    }

    fn has_turn(&self, request: &str) -> bool {
        self.0.is_some_and(|round| {
            round.marks.contains_key(request) || round.turns.contains_key(request)
        })
    }
}

impl FileMarkers<'_> {
    pub(crate) fn marker(&self, author: &MarkAuthor, line: Line) -> Marker {
        match author {
            MarkAuthor::Reviewer => Marker::ByHand,
            MarkAuthor::Jev => Marker::Jev,
            MarkAuthor::ExploreRead { request } if self.round.has_turn(request) => {
                Marker::NotRelevant
            }
            MarkAuthor::Explore { answer } if self.round.has_answer(answer) => {
                if self.found_not_relevant(answer, line) {
                    Marker::NotRelevant
                } else {
                    Marker::Answer
                }
            }
            MarkAuthor::ExploreRead { .. } | MarkAuthor::Explore { .. } => Marker::OtherRound,
        }
    }

    fn found_not_relevant(&self, answer: &str, line: Line) -> bool {
        self.not_relevant
            .iter()
            .any(|(applied, named)| *applied == answer && line.in_lines(named))
    }
}

impl Line {
    fn in_lines(self, named: &NamedLines) -> bool {
        match self {
            Self::One(side, line) => named.contains(side, line),
            Self::Whole => true,
        }
    }
}

pub(crate) struct Answering<'a> {
    question: &'a InterviewUpdate,
    markable: &'a MarkableFiles,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct AnswerEffect {
    pub(crate) pending: PendingLines,
    pub(crate) reopened: u64,
}

struct FileAnswer {
    reviewed: Option<NamedLines>,
    not_relevant: Option<NamedLines>,
    reopened: Option<NamedLines>,
}

impl<'a> Answering<'a> {
    pub(crate) fn new(question: &'a InterviewUpdate, markable: &'a MarkableFiles) -> Self {
        Self { question, markable }
    }

    pub(crate) fn cites(&self, file: &ChangedFile) -> bool {
        self.question.next.as_ref().is_some_and(|question| {
            question
                .evidence
                .iter()
                .any(|evidence| evidence.location.names(file))
        })
    }

    fn in_file(&self, file: &ChangedFile) -> FileAnswer {
        if !self.markable.allows(file) {
            return FileAnswer {
                reviewed: None,
                not_relevant: None,
                reopened: None,
            };
        }
        let question = self.question;
        FileAnswer {
            reviewed: NamedLines::in_file(file, &question.reviewed),
            not_relevant: NamedLines::in_file(
                file,
                NotRelevantMark::locations(&question.not_relevant),
            ),
            reopened: NamedLines::in_file(file, &question.reopened),
        }
    }

    pub(crate) fn on_lines(
        &self,
        file: &ChangedFile,
        open: &LineSelection,
        reviewed: &ReviewedLines,
    ) -> AnswerEffect {
        let answer = self.in_file(file);
        let mut pending = PendingLines::default();
        for line in lines(open.removed.iter().copied(), open.added.iter().copied()) {
            if answer.marks_reviewed(line) {
                pending.reviewed += 1;
            } else if FileAnswer::names(answer.not_relevant.as_ref(), line) {
                pending.not_relevant += 1;
            }
        }
        let reopened = lines(
            reviewed.removed.keys().copied(),
            reviewed.added.keys().copied(),
        )
        .filter(|line| answer.reopens(*line))
        .count();
        AnswerEffect {
            pending,
            reopened: count(reopened),
        }
    }

    pub(crate) fn on_whole(&self, file: &ChangedFile, marked: bool) -> Option<WholeChange> {
        let answer = self.in_file(file);
        if marked {
            return answer.reopens(Line::Whole).then_some(WholeChange::Reopened);
        }
        if answer.reviewed.is_some() {
            Some(WholeChange::Reviewed)
        } else {
            answer
                .not_relevant
                .is_some()
                .then_some(WholeChange::NotRelevant)
        }
    }
}

impl FileAnswer {
    fn names(named: Option<&NamedLines>, line: Line) -> bool {
        named.is_some_and(|named| line.in_lines(named))
    }

    fn marks_reviewed(&self, line: Line) -> bool {
        Self::names(self.reviewed.as_ref(), line)
    }

    fn reopens(&self, line: Line) -> bool {
        Self::names(self.reopened.as_ref(), line)
            && !self.marks_reviewed(line)
            && !Self::names(self.not_relevant.as_ref(), line)
    }
}

fn lines(
    removed: impl Iterator<Item = u32>,
    added: impl Iterator<Item = u32>,
) -> impl Iterator<Item = Line> {
    removed
        .map(|line| Line::One(SourceSide::Old, line))
        .chain(added.map(|line| Line::One(SourceSide::New, line)))
}
