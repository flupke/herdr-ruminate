use review_explore::{Conclusion, Decision, DecisionTag, QuizItem};
use review_explore_citations::Citation;
use serde::Serialize;
use ts_rs::TS;

use super::question::CitationView;
use super::{markdown, markdown_if_any};
use crate::round::{ImplementationState, PageImplementation, PageQuiz, RoundSnapshot, RoundStage};
use crate::status::StatusCard;

#[derive(Debug, Serialize, TS)]
pub(crate) struct ConclusionView {
    request: String,
    lead_html: Option<String>,
    summary_html: String,
    decisions: Vec<DecisionView>,
    future_work_html: Option<String>,
    draft: String,
    list: ListView,
    implementation: Option<ImplementationView>,
    implementation_card: Option<StatusCard>,
    reply_label: Option<&'static str>,
    posts_review: bool,
    quiz: Option<QuizView>,
}

#[derive(Debug, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ListView {
    Editable,
    Draft {
        html: String,
    },
    Request {
        label: &'static str,
        settled: bool,
        edit: bool,
        new_round: bool,
    },
}

#[derive(Debug, Serialize, TS)]
struct DecisionView {
    number: usize,
    question_html: String,
    choice: Option<String>,
    comment: String,
    #[ts(type = "Array<\"changed_after_first_pick\" | \"as_recommended\">")]
    tags: Vec<DecisionTag>,
}

#[derive(Debug, Serialize, TS)]
struct ImplementationView {
    delivery: String,
    text_html: String,
}

#[derive(Debug, Serialize, TS)]
struct QuizView {
    items: Vec<QuizItemView>,
    next: Option<usize>,
    correct_picks: usize,
    picked: usize,
    skipped: bool,
}

#[derive(Debug, Serialize, TS)]
struct QuizItemView {
    number: usize,
    question: String,
    answers: Vec<QuizAnswerView>,
    picked_correct: Option<bool>,
    correct_answer: String,
    why: String,
    proof: Vec<CitationView>,
}

#[derive(Debug, Serialize, TS)]
struct QuizAnswerView {
    index: usize,
    text: String,
    correct: bool,
    picked: bool,
}

impl ConclusionView {
    pub(crate) fn of(round: &RoundSnapshot, offers_actions: bool) -> Option<Self> {
        let RoundStage::Conclusion {
            request,
            conclusion,
            implementation,
            quiz,
            ..
        } = &round.stage
        else {
            return None;
        };
        let decisions = round
            .overview
            .as_deref()
            .map_or(&[][..], |overview| &overview.decisions[..]);
        Some(Self::new(
            round,
            request,
            conclusion,
            implementation.as_ref(),
            quiz,
            decisions,
            offers_actions,
        ))
    }

    fn new(
        round: &RoundSnapshot,
        request: &str,
        conclusion: &Conclusion,
        implementation: Option<&PageImplementation>,
        quiz: &PageQuiz,
        decisions: &[Decision],
        offers_actions: bool,
    ) -> Self {
        let replaces = implementation.map(|implementation| implementation.delivery.as_str());
        let offers_implement = offers_actions && round.stage.offers_implement(request, replaces);
        let posts_review = round.author == review_explore::ChangeAuthor::Other;
        let draft = match implementation {
            Some(implementation) if offers_implement => &implementation.text,
            _ => &conclusion.to_be_implemented,
        };
        let (lead_html, summary_html) = lead(markdown(&conclusion.summary, 2));
        Self {
            request: request.to_owned(),
            lead_html,
            summary_html,
            decisions: decisions.iter().map(DecisionView::new).collect(),
            future_work_html: markdown_if_any(&conclusion.future_work, 3),
            draft: draft.clone(),
            list: ListView::new(draft, implementation, offers_actions, offers_implement),
            implementation: implementation.map(|implementation| ImplementationView {
                delivery: implementation.delivery.clone(),
                text_html: markdown(&implementation.text, 3),
            }),
            implementation_card: implementation.map(|implementation| {
                StatusCard::implementation(
                    implementation,
                    request,
                    offers_actions,
                    offers_implement,
                    posts_review,
                )
            }),
            reply_label: offers_actions.then_some(match implementation.map(|i| &i.state) {
                Some(ImplementationState::Sent) => "Reply to the agent",
                _ => "Not ready? Reply to the agent instead",
            }),
            quiz: (!conclusion.quiz.is_empty()).then(|| QuizView::new(&conclusion.quiz, quiz)),
            posts_review,
        }
    }
}

impl ListView {
    fn new(
        draft: &str,
        implementation: Option<&PageImplementation>,
        offers_actions: bool,
        offers_implement: bool,
    ) -> Self {
        use ImplementationState::{Cancelled, NotSent, NotStarted, Paused, Sending, Sent, Unknown};
        let request = |label, settled| Self::Request {
            label,
            settled,
            edit: false,
            new_round: false,
        };
        let Some(implementation) = implementation else {
            return if offers_implement {
                Self::Editable
            } else {
                Self::Draft {
                    html: markdown(draft, 3),
                }
            };
        };
        if !offers_actions {
            return request("The request", true);
        }
        match implementation.state {
            Paused => Self::Request {
                label: "Saved request",
                settled: false,
                edit: offers_implement,
                new_round: false,
            },
            NotStarted => request("The request", false),
            Sent => Self::Request {
                label: "Sent",
                settled: true,
                edit: false,
                new_round: true,
            },
            NotSent(_) | Cancelled if offers_implement => Self::Editable,
            Unknown | Sending | NotSent(_) | Cancelled => request("The request", true),
        }
    }
}

fn lead(html: String) -> (Option<String>, String) {
    const END: &str = "</p>";
    match html.find(END) {
        Some(end) if html.starts_with("<p>") => {
            let (lead, rest) = html.split_at(end + END.len());
            (Some(lead.to_owned()), rest.trim_start().to_owned())
        }
        _ => (None, html),
    }
}

impl DecisionView {
    fn new(decision: &Decision) -> Self {
        Self {
            number: decision.number,
            question_html: markdown(&decision.question, 3),
            choice: decision.answer.choice.clone(),
            comment: decision.answer.comment.clone(),
            tags: decision.answer.tags.clone(),
        }
    }
}

impl QuizView {
    fn new(items: &[QuizItem], quiz: &PageQuiz) -> Self {
        let answers = &quiz.answers;
        Self {
            items: items
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    let proof = quiz.proofs.get(index).map_or(&[][..], |proof| &proof[..]);
                    QuizItemView::new(index, item, proof, answers.pick(index).map(|p| p.answer))
                })
                .collect(),
            next: answers
                .next_item(items.len())
                .filter(|_| quiz.takes_answers),
            correct_picks: answers.correct_picks(),
            picked: answers.picks.len(),
            skipped: answers.skipped,
        }
    }
}

impl QuizItemView {
    fn new(index: usize, item: &QuizItem, proof: &[Citation], picked: Option<usize>) -> Self {
        Self {
            number: index + 1,
            question: item.question.clone(),
            answers: item
                .answers
                .iter()
                .enumerate()
                .map(|(option, text)| QuizAnswerView {
                    index: option,
                    text: text.clone(),
                    correct: option == item.correct,
                    picked: picked == Some(option),
                })
                .collect(),
            picked_correct: picked.map(|picked| picked == item.correct),
            correct_answer: item.answers.get(item.correct).cloned().unwrap_or_default(),
            why: item.why.clone(),
            proof: proof.iter().map(CitationView::new).collect(),
        }
    }
}
