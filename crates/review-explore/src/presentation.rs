use review_types::TextEditorState;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
pub enum ExplorePage {
    Opening,
    Question(usize),
    Conclusion(String),
}

impl Default for ExplorePage {
    fn default() -> Self {
        Self::Question(0)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub struct QuestionReading {
    pub reference: usize,
    pub choice: usize,
    pub more: bool,
    pub references: bool,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub enum EditorFocus {
    #[default]
    Answer,
    Implementation,
    Evidence,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub enum RoundFront {
    #[default]
    Pane,
    Page,
}

impl RoundFront {
    #[allow(
        clippy::trivially_copy_pass_by_ref,
        reason = "serde's skip_serializing_if passes the field by reference"
    )]
    fn is_pane(&self) -> bool {
        *self == Self::Pane
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub struct EvidencePosition {
    pub turn: usize,
    pub reference: usize,
    pub scroll: usize,
    pub cursor: usize,
    pub column: usize,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Independent saved expansion and editor states"
)]
pub struct ExploreViewState {
    pub page: ExplorePage,
    pub turns: Vec<QuestionReading>,
    pub drafts: Vec<(ExplorePage, TextEditorState)>,
    pub tasks: BTreeMap<String, TextEditorState>,
    pub replies: BTreeMap<String, bool>,
    pub editing: bool,
    pub focus: EditorFocus,
    pub scroll: usize,
    pub map: bool,
    pub heights: Vec<((usize, usize), u16)>,
    pub code: Vec<EvidencePosition>,
    #[serde(default, skip_serializing_if = "RoundFront::is_pane")]
    pub front: RoundFront,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct ViewSave {
    pub review_unit: review_types::ReviewUnit,
    pub instance: String,
    pub sequence: u64,
    pub state: ExploreViewState,
}

impl crate::Exploration {
    pub fn answer_page(&self, answer: &crate::ReviewerAnswer) -> Option<ExplorePage> {
        match &answer.question {
            Some(question) => self
                .questions
                .iter()
                .position(|q| q == question)
                .map(ExplorePage::Question),
            None => Some(ExplorePage::Conclusion(answer.in_reply_to.clone())),
        }
    }
}

impl crate::ExploreRound {
    pub fn restored_view(&self, view: Option<&ViewSave>) -> ExploreViewState {
        let Some(view) = view else {
            return ExploreViewState {
                page: self.exploration.conclusion_request().map_or_else(
                    || {
                        self.exploration
                            .questions
                            .len()
                            .checked_sub(1)
                            .map_or(ExplorePage::Question(0), ExplorePage::Question)
                    },
                    |id| ExplorePage::Conclusion(id.to_owned()),
                ),
                ..Default::default()
            };
        };
        let mut state = view.state.clone();
        for delivery in self.turns.values() {
            if delivery
                .editor_sequence
                .is_some_and(|sequence| view.sequence <= sequence)
                && let Some(answer) = &delivery.request.answer
            {
                let target = self.exploration.answer_page(answer);
                state.drafts.retain(|(page, draft)| {
                    Some(page) != target.as_ref() || draft.text != answer.text
                });
            }
        }
        state
    }
}
