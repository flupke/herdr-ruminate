mod conclusion;
mod conversation;
mod earlier;
mod question;
mod sent;

use change_references::ChangeReferences;
use markdown_html::HtmlRenderer;
use review_explore::{AgentRecord, Design, QuestionSection, RailStep, TabTitle};
use review_explore_tally::MarkTally;
use serde::Serialize;
use ts_rs::TS;

pub(crate) use self::conclusion::ConclusionView;
use self::conversation::ConversationView;
use self::earlier::EarlierQuestionView;
pub(crate) use self::question::QuestionView;
use self::sent::SentView;
use crate::blind::FirstPicks;
use crate::conversation::ThreadsSnapshot;
use crate::past::{PastChange, PastRound};
use crate::round::{LatestAnswer, ReviewName, RoundSnapshot, RoundStage, TurnResponse};
use crate::status::StatusCard;

#[derive(Debug, Serialize, TS)]
pub(crate) struct PageView {
    review: Option<ReviewName>,
    start: Option<StartView>,
    cards: Vec<StatusCard>,
    cancellable: Option<LatestAnswer>,
    answered: Option<usize>,
    design: Option<DesignView>,
    response: Option<ResponseView>,
    question: Option<QuestionView>,
    conclusion: Option<ConclusionView>,
    reset: Option<String>,
    earlier: bool,
    mermaid: String,
    rail: Vec<RailStep>,
    title: Option<TabTitle>,
    tally: Option<MarkTally>,
    earlier_questions: Vec<EarlierQuestionView>,
    sent: Option<SentView>,
    conversation: Option<ConversationView>,
    glossary: std::collections::BTreeMap<String, String>,
    past: Option<PastView>,
    past_rounds: Option<Vec<PastChange>>,
}

#[derive(Debug, Serialize, TS)]
struct PastView {
    round: String,
    saved_at_ms: u64,
    current_waits: bool,
    references: ChangeReferences,
}

#[derive(Debug, Serialize, TS)]
struct StartView {
    start: String,
    idle: bool,
    block: Option<String>,
}

#[derive(Debug, Serialize, TS)]
pub(crate) struct SectionView {
    title: String,
    lead_html: String,
    details_html: Option<String>,
}

impl SectionView {
    pub(crate) fn new(section: &QuestionSection, level: usize) -> Self {
        Self {
            title: section.title.to_owned(),
            lead_html: markdown(&section.lead, level),
            details_html: markdown_if_any(&section.details, level),
        }
    }
}

#[derive(Debug, Serialize, TS)]
struct DesignView {
    thesis_html: String,
    parts: Vec<DesignPartView>,
    minutes: usize,
    changed_files: usize,
}

#[derive(Debug, Serialize, TS)]
struct DesignPartView {
    title: String,
    thesis_html: String,
    body_html: String,
}

#[derive(Debug, Serialize, TS)]
struct ResponseView {
    interpretations: Vec<InterpretationView>,
    reply_html: Option<String>,
    path_line: Option<String>,
    marks_problem: Option<String>,
}

#[derive(Debug, Serialize, TS)]
struct InterpretationView {
    recap_html: Option<String>,
    follow_ups: Vec<String>,
}

impl PageView {
    pub(crate) fn new(
        round: &RoundSnapshot,
        picks: &FirstPicks,
        threads: Option<&ThreadsSnapshot>,
    ) -> Self {
        let offers_actions = !round.earlier;
        let cards = StatusCard::above_stage(round, offers_actions);
        let start = round.stage.offered_start().map(|start| StartView {
            start: start.to_owned(),
            idle: !StatusCard::say_start(&cards),
            block: StatusCard::start_block(&cards).map(str::to_owned),
        });
        let storage_failed = matches!(round.stage, RoundStage::StorageFailed { .. });
        Self {
            review: round.review.clone(),
            start,
            cancellable: round
                .cancellable
                .clone()
                .filter(|_| offers_actions && !storage_failed),
            answered: round.answered_number(),
            design: round
                .design
                .as_deref()
                .map(|design| DesignView::new(design, round.changed_files)),
            response: round.stage.response().map(ResponseView::new),
            question: QuestionView::of(round, picks),
            conclusion: ConclusionView::of(round, offers_actions),
            reset: round
                .round
                .clone()
                .filter(|_| !storage_failed && !round.past),
            earlier: round.earlier,
            cards,
            mermaid: crate::diagram::script_path(),
            rail: round
                .overview
                .as_ref()
                .map_or_else(Vec::new, |overview| overview.rail.clone()),
            title: round
                .overview
                .as_ref()
                .filter(|_| !storage_failed)
                .map(|overview| overview.title),
            earlier_questions: round.overview.as_ref().map_or_else(Vec::new, |overview| {
                let cancellable = offers_actions && round.cancellable.is_some();
                EarlierQuestionView::all(&overview.earlier, &round.earlier_citations, cancellable)
            }),
            tally: round.tally.as_deref().cloned(),
            sent: SentView::of(round, offers_actions),
            conversation: threads.and_then(|threads| ConversationView::of(round, threads)),
            glossary: round
                .overview
                .as_ref()
                .map(|overview| overview.glossary.clone())
                .unwrap_or_default(),
            past: None,
            past_rounds: None,
        }
    }

    pub(crate) fn of_past(
        past: &PastRound,
        followed: &RoundSnapshot,
        live: Option<&ThreadsSnapshot>,
    ) -> Self {
        let threads = live
            .filter(|live| {
                past.snapshot
                    .review_unit
                    .as_ref()
                    .is_some_and(|unit| live.threads(unit).is_some())
            })
            .unwrap_or(&past.threads);
        Self {
            past: Some(PastView {
                round: past.id().to_owned(),
                saved_at_ms: past.saved_at_ms,
                current_waits: followed.waits_for_reviewer(),
                references: past.references.clone(),
            }),
            ..Self::new(&past.snapshot, &FirstPicks::default(), Some(threads))
        }
    }

    pub(crate) fn listing(self, past_rounds: Option<Vec<PastChange>>) -> Self {
        Self {
            past_rounds,
            ..self
        }
    }
}

impl DesignView {
    fn new(design: &Design, changed_files: usize) -> Self {
        let inline = |text: &str| {
            HtmlRenderer::under_heading(3)
                .render_inline(text)
                .into_string()
        };
        Self {
            thesis_html: inline(&design.thesis()),
            parts: design
                .parts()
                .into_iter()
                .map(|part| DesignPartView {
                    title: part.title.to_owned(),
                    thesis_html: inline(&part.thesis),
                    body_html: markdown(&part.body, 3),
                })
                .collect(),
            minutes: design.reading_minutes(),
            changed_files,
        }
    }
}

impl ResponseView {
    fn of_record(record: &AgentRecord) -> Self {
        Self::new(&TurnResponse {
            interpretations: record.interpretation.iter().cloned().collect(),
            reply: record.reply.clone().filter(|text| !text.trim().is_empty()),
            path: None,
            revision: None,
            marks_problem: None,
        })
    }

    fn new(response: &TurnResponse) -> Self {
        Self {
            interpretations: response
                .interpretations
                .iter()
                .map(|interpretation| InterpretationView {
                    recap_html: markdown_if_any(&interpretation.recap, 2),
                    follow_ups: interpretation.follow_ups.clone(),
                })
                .collect(),
            reply_html: response.reply.as_deref().map(|reply| markdown(reply, 2)),
            path_line: response.path_line().map(std::borrow::Cow::into_owned),
            marks_problem: response.marks_problem.clone(),
        }
    }
}

pub(crate) fn markdown(text: &str, level: usize) -> String {
    HtmlRenderer::under_heading(level)
        .render(text)
        .into_string()
}

pub(crate) fn markdown_if_any(text: &str, level: usize) -> Option<String> {
    (!text.is_empty()).then(|| markdown(text, level))
}
