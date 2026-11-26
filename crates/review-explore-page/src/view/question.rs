use review_explore::{Alternative, Assessments, Door, MarkCounts, MarkPhrase, MarkTense, Question};
use review_explore_citations::{Citation, CodeColors, HighlightedRow, Token};
use review_explore_tally::{Gain, MarkTally, WholeChange};
use review_repository::diff::DiffRow;
use serde::Serialize;
use ts_rs::TS;

use super::{SectionView, markdown, markdown_if_any};
use crate::blind::{BlindQuestion, FirstPicks};
use crate::round::{AnsweredQuestion, QuestionMarks, RoundSnapshot, RoundStage};

#[derive(Debug, Serialize, TS)]
pub(crate) struct QuestionView {
    round: Option<String>,
    number: usize,
    id: String,
    version: u32,
    text_html: String,
    door: Option<Door>,
    context_html: Option<String>,
    sections: Vec<SectionView>,
    recommendation: Recommendation,
    choices: Vec<ChoiceView>,
    citations: Vec<CitationView>,
    marks: Option<MarksView>,
    gain: Option<GainView>,
    answerable: bool,
    revision: Option<RevisionView>,
}

#[derive(Debug, Serialize, TS)]
struct RevisionView {
    reason_html: String,
    replaced_version: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
enum Recommendation {
    Shown,
    HiddenUntilPick,
    ShownAfterPick,
}

#[derive(Debug, Serialize, TS)]
pub(super) struct ChoiceView {
    id: String,
    text: String,
    recommendation: Option<String>,
    checked: bool,
}

#[derive(Debug, Serialize, TS)]
struct MarksView {
    summary: MarkPhrase,
    reviewed: Vec<String>,
    not_relevant: Vec<String>,
    reopened: Vec<String>,
}

#[derive(Debug, Serialize, TS)]
struct GainView {
    before: u64,
    after: u64,
}

#[derive(Debug, Serialize, TS)]
pub(crate) struct CitationView {
    location: String,
    path: String,
    span: String,
    notes: String,
    rows: Vec<RowView>,
    limitation: Option<String>,
}

#[derive(Debug, Serialize, TS)]
struct RowView {
    kind: RowKind,
    old_line: Option<u32>,
    new_line: Option<u32>,
    tokens: Vec<TokenView>,
}

#[derive(Clone, Copy, Debug, Serialize, TS)]
#[serde(rename_all = "snake_case")]
enum RowKind {
    Context,
    Added,
    Removed,
}

#[derive(Debug, Serialize, TS)]
struct TokenView {
    text: String,
    role: Option<String>,
}

impl QuestionView {
    pub(crate) fn of(round: &RoundSnapshot, picks: &FirstPicks) -> Option<Self> {
        let RoundStage::Question {
            question,
            citations,
            marks,
            response,
            ..
        } = &round.stage
        else {
            return None;
        };
        let blind = round.stage.blind().filter(|_| !round.past);
        let picked = blind.as_ref().and_then(|blind| {
            let round = round.round.as_deref()?;
            picks
                .of(round, &question.id, question.version)
                .filter(|choice| blind.offers(choice))
        });
        Some(
            Self::new(
                round.round.clone(),
                round.question_number().unwrap_or_default(),
                question,
                blind.as_ref(),
                citations,
                marks,
                picked.as_deref(),
            )
            .answerable(!round.earlier)
            .counted(round.tally.as_deref())
            .revised(response.revision.as_ref()),
        )
    }

    pub(crate) fn answered(
        round: Option<String>,
        number: usize,
        answered: &AnsweredQuestion,
    ) -> Self {
        let mut view = Self::new(
            round,
            number,
            &answered.question,
            None,
            &answered.citations,
            &QuestionMarks::default(),
            None,
        )
        .answerable(false);
        if answered.picked_blind {
            view.recommendation = Recommendation::ShownAfterPick;
        }
        view
    }

    fn new(
        round: Option<String>,
        number: usize,
        question: &Question,
        blind: Option<&BlindQuestion<'_>>,
        citations: &[Citation],
        marks: &QuestionMarks,
        picked: Option<&str>,
    ) -> Self {
        let (recommendation, choices) = match (blind, picked) {
            (None, _) => (Recommendation::Shown, question.choices().collect()),
            (Some(blind), None) => (Recommendation::HiddenUntilPick, blind.choices()),
            (Some(blind), Some(_)) => (Recommendation::ShownAfterPick, blind.choices()),
        };
        let choices: Vec<_> = choices
            .into_iter()
            .map(|choice| ChoiceView::new(choice, recommendation, picked))
            .collect();
        Self {
            round,
            number,
            id: question.id.clone(),
            version: question.version,
            text_html: markdown(&question.text, 2),
            door: question
                .assessments
                .as_ref()
                .map(|assessments| assessments.door),
            context_html: markdown_if_any(&question.context(), 2),
            sections: question
                .assessments
                .iter()
                .flat_map(Assessments::sections)
                .map(|section| SectionView::new(&section, 2))
                .collect(),
            recommendation,
            choices,
            citations: citations.iter().map(CitationView::new).collect(),
            marks: MarksView::new(marks),
            gain: None,
            answerable: true,
            revision: None,
        }
    }

    fn revised(mut self, revision: Option<&crate::round::QuestionRevised>) -> Self {
        self.revision = revision.map(|revision| RevisionView {
            reason_html: markdown(&revision.reason, 2),
            replaced_version: revision.replaced,
        });
        self
    }

    fn answerable(mut self, answerable: bool) -> Self {
        self.answerable = answerable;
        self
    }

    pub(super) fn keeping(mut self, kept: Option<&str>) -> Self {
        for choice in &mut self.choices {
            choice.keep(kept);
        }
        self
    }

    fn counted(mut self, tally: Option<&MarkTally>) -> Self {
        let Some((tally, gain)) = tally.and_then(|tally| Some((tally, tally.gain.as_ref()?)))
        else {
            return self;
        };
        let Some(marks) = &mut self.marks else {
            return self;
        };
        let summary = Self::answering(tally, gain);
        if summary.parts.is_empty() {
            self.marks = None;
            return self;
        }
        marks.summary = summary;
        self.gain = Some(GainView {
            before: gain.before.percent,
            after: gain.after.percent,
        });
        self
    }

    fn answering(tally: &MarkTally, gain: &Gain) -> MarkPhrase {
        let lines = |count: u64| u32::try_from(count).unwrap_or(u32::MAX);
        let whole = |change: WholeChange| {
            tally
                .files
                .iter()
                .filter(|file| file.whole.and_then(|whole| whole.answering) == Some(change))
                .count()
        };
        let files = |count: usize| u32::try_from(count).unwrap_or(u32::MAX);
        MarkCounts {
            reviewed_lines: lines(gain.pending.reviewed),
            reviewed_files: files(whole(WholeChange::Reviewed)),
            not_relevant_lines: lines(gain.pending.not_relevant),
            not_relevant_files: files(whole(WholeChange::NotRelevant)),
            reopened_lines: lines(gain.reopened),
            reopened_files: files(whole(WholeChange::Reopened)),
        }
        .covered_phrase(MarkTense::Answering)
    }
}

impl ChoiceView {
    pub(super) fn kept(question: &Question, kept: Option<&str>) -> Vec<Self> {
        question
            .choices()
            .map(|choice| {
                let mut view = Self::new(choice, Recommendation::Shown, None);
                view.keep(kept);
                view
            })
            .collect()
    }

    fn keep(&mut self, kept: Option<&str>) {
        self.checked = kept == Some(self.text.as_str());
    }

    fn new(choice: &Alternative, recommendation: Recommendation, first_pick: Option<&str>) -> Self {
        Self {
            id: choice.id.clone(),
            text: choice.text.clone(),
            recommendation: choice
                .recommendation
                .clone()
                .filter(|_| recommendation != Recommendation::HiddenUntilPick),
            checked: first_pick == Some(choice.id.as_str()),
        }
    }
}

impl MarksView {
    fn new(marks: &QuestionMarks) -> Option<Self> {
        let summary = marks.counts().covered_phrase(MarkTense::Answering);
        if summary.parts.is_empty() {
            return None;
        }
        Some(Self {
            summary,
            reviewed: marks.reviewed.iter().map(ToString::to_string).collect(),
            not_relevant: marks.not_relevant.iter().map(ToString::to_string).collect(),
            reopened: marks.reopened.iter().map(ToString::to_string).collect(),
        })
    }
}

impl CitationView {
    pub(crate) fn new(citation: &Citation) -> Self {
        let (rows, limitation) = match &citation.lines {
            Ok(rows) => (
                RowView::dedented(rows.iter().filter_map(RowView::new).collect()),
                None,
            ),
            Err(limitation) => (Vec::new(), Some(limitation.to_string())),
        };
        let location = &citation.evidence.location;
        Self {
            location: location.to_string(),
            path: location.path.display().to_string(),
            span: location.lines.as_ref().map_or_else(
                || "whole file".to_owned(),
                |lines| {
                    let span = Self::span(lines.first_line, lines.last_line);
                    format!("{} {span}", location.side)
                },
            ),
            notes: citation.evidence.notes.clone(),
            rows,
            limitation,
        }
    }
}

impl RowView {
    fn new(row: &HighlightedRow) -> Option<Self> {
        let (kind, old_line, new_line) = match row.diff {
            DiffRow::Context {
                old_line, new_line, ..
            } => (RowKind::Context, Some(old_line), Some(new_line)),
            DiffRow::Delete { old_line, .. } => (RowKind::Removed, Some(old_line), None),
            DiffRow::Add { new_line, .. } => (RowKind::Added, None, Some(new_line)),
            _ => return None,
        };
        Some(Self {
            kind,
            old_line,
            new_line,
            tokens: row.tokens.iter().map(TokenView::new).collect(),
        })
    }
}

impl CitationView {
    fn span(first: u32, last: u32) -> String {
        if first == last {
            first.to_string()
        } else {
            format!("{first}–{last}")
        }
    }
}

impl RowView {
    fn dedented(mut rows: Vec<Self>) -> Vec<Self> {
        let shared = rows.iter().filter_map(Self::indentation).min().unwrap_or(0);
        if shared > 0 {
            for row in &mut rows {
                row.strip(shared);
            }
        }
        rows
    }

    fn indentation(&self) -> Option<usize> {
        let mut count = 0;
        for character in self.tokens.iter().flat_map(|token| token.text.chars()) {
            match character {
                ' ' | '\t' => count += 1,
                '\n' | '\r' => return None,
                _ => return Some(count),
            }
        }
        None
    }

    fn strip(&mut self, mut count: usize) {
        for token in &mut self.tokens {
            if count == 0 {
                break;
            }
            let blank = token
                .text
                .chars()
                .take(count)
                .take_while(|character| matches!(character, ' ' | '\t'))
                .count();
            token.text.drain(..blank);
            count -= blank;
            if !token.text.is_empty() {
                break;
            }
        }
        self.tokens.retain(|token| !token.text.is_empty());
    }
}

impl TokenView {
    fn new(token: &Token) -> Self {
        Self {
            text: token.text.clone(),
            role: CodeColors::role(token).map(str::to_owned),
        }
    }
}

#[cfg(test)]
#[path = "question.tests.rs"]
mod tests;
