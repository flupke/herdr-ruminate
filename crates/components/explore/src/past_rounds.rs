use review_explore_page_opening::PageScreen;
use ui_actions::Action;
use ui_controls::KeyHint;
use ui_shortcuts::ExploreTurnShortcut;

use super::start::key_hint;
use super::{Control, ExploreComponent, Progress, flow::ConversationLayout};

impl ExploreComponent {
    fn offers_past_rounds(&self) -> bool {
        self.exploration.is_none() && matches!(self.progress, Progress::Ready | Progress::Retryable)
    }

    pub(super) fn open_past_rounds(&mut self) -> Vec<Action> {
        if !self.offers_past_rounds() {
            return Vec::new();
        }
        self.open_page(PageScreen::PastRounds)
    }

    pub(super) fn lay_out_past_rounds(&self, layout: &mut ConversationLayout) {
        if !self.offers_past_rounds() {
            return;
        }
        layout.gap();
        layout.controls([(
            "Past rounds on the Explore page".into(),
            Control::PastRounds,
        )]);
    }

    pub(super) fn past_rounds_hint(&self) -> Option<KeyHint> {
        self.offers_past_rounds()
            .then(|| key_hint(ExploreTurnShortcut::PastRounds, "past rounds"))
            .flatten()
    }
}
