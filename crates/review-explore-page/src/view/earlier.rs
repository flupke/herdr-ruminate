use std::sync::Arc;

use review_explore::{
    Assessments, CancelFrom, Door, EarlierQuestion, KeptAnswer, MarkPhrase, MarkTense,
};
use review_explore_citations::Citation;
use serde::Serialize;
use ts_rs::TS;

use super::question::{ChoiceView, CitationView};
use super::{ResponseView, SectionView, markdown, markdown_if_any};

#[derive(Debug, Serialize, TS)]
pub(crate) struct EarlierQuestionView {
    number: usize,
    text_html: String,
    door: Option<Door>,
    context_html: Option<String>,
    sections: Vec<SectionView>,
    choices: Vec<ChoiceView>,
    answer: Option<KeptAnswer>,
    recorded: ResponseView,
    marks: Vec<MarkPhrase>,
    marks_problems: Vec<String>,
    citations: Vec<CitationView>,
    id: String,
    version: u32,
    cancel: Option<CancelFrom>,
}

impl EarlierQuestionView {
    fn new(record: &EarlierQuestion, citations: &[Citation], cancellable: bool) -> Self {
        let question = &record.question;
        Self {
            number: record.number,
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
            choices: ChoiceView::kept(
                question,
                record
                    .answer
                    .as_ref()
                    .and_then(|answer| answer.choice.as_deref()),
            ),
            answer: record.answer.clone(),
            recorded: ResponseView::of_record(&record.recorded),
            marks: record
                .marks
                .iter()
                .map(|marks| marks.counts().covered_phrase(MarkTense::Applied))
                .filter(|phrase| !phrase.parts.is_empty())
                .collect(),
            marks_problems: record
                .marks
                .iter()
                .filter_map(|marks| marks.problem.clone())
                .collect(),
            citations: citations.iter().map(CitationView::new).collect(),
            id: question.id.clone(),
            version: question.version,
            cancel: record.cancel.clone().filter(|_| cancellable),
        }
    }

    pub(crate) fn all(
        records: &[EarlierQuestion],
        citations: &[Arc<[Citation]>],
        cancellable: bool,
    ) -> Vec<Self> {
        records
            .iter()
            .enumerate()
            .map(|(index, record)| {
                let citations = citations.get(index).map_or(&[][..], |list| list);
                Self::new(record, citations, cancellable)
            })
            .collect()
    }
}
