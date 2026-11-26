use review_explore::RoundFront;
use review_explore_page_opening::PageScreen;
use ui_actions::{Action, ExplorePageAction};
use ui_controls::KeyHint;
use ui_shortcuts::{ExploreCommand, ExploreShortcut, ExploreTurnShortcut};
use ui_theme::Palette;

use super::start::{FrontControl, PageRoundButton, StartButton};
use super::{Control, ExploreComponent, Progress, flow::ConversationLayout};

impl ExploreComponent {
    pub(super) fn shows_page_round(&self) -> bool {
        self.front == RoundFront::Page
            && (self.exploration.is_some()
                || self.progress.awaiting_capture()
                || self.awaiting_page_start())
    }

    pub(super) fn front_control(&mut self, control: FrontControl) -> Vec<Action> {
        match control {
            FrontControl::Start(start) => self.start(start),
            FrontControl::OpenPage if self.shows_page_round() => {
                self.open_page(PageScreen::Current)
            }
            FrontControl::ContinueInPane if self.shows_page_round() => {
                self.continue_in_pane();
                Vec::new()
            }
            FrontControl::OpenPage | FrontControl::ContinueInPane => Vec::new(),
        }
    }

    pub(super) fn page_round_key(&mut self, command: Option<ExploreCommand>) -> Vec<Action> {
        let Some(ExploreCommand::Explore(ExploreShortcut::Turn(shortcut))) = command else {
            return Vec::new();
        };
        if let ExploreTurnShortcut::Setting(setting) = shortcut {
            return self.setting(setting);
        }
        PageRoundButton::of(shortcut)
            .map(|button| self.front_control(button.control))
            .unwrap_or_default()
    }

    pub fn footer_hints(&self) -> Vec<KeyHint> {
        if self.page_settings.editing() {
            Vec::new()
        } else if self.shows_page_round() {
            PageRoundButton::hints()
        } else {
            let starts = if self.offers_starts() {
                StartButton::hints(self.start_block)
            } else {
                Vec::new()
            };
            starts.into_iter().chain(self.past_rounds_hint()).collect()
        }
    }

    pub(super) fn open_page(&mut self, screen: PageScreen) -> Vec<Action> {
        self.unopened_page = None;
        vec![Action::ExplorePage(ExplorePageAction::Open(screen))]
    }

    fn continue_in_pane(&mut self) {
        self.front = RoundFront::Pane;
        self.open_page = false;
        if self.exploration.is_some() {
            self.reveal.set(Some(super::Reveal::Start));
            self.publish_evidence(self.view_id(), false);
        }
    }

    pub(super) fn lay_out_page_round(&self, layout: &mut ConversationLayout, palette: Palette) {
        layout.text(
            "This Explore round runs on the Explore page.",
            palette.text,
            None,
        );
        layout.text(self.page_round_state(), palette.dim, None);
        layout.gap();
        layout.controls(
            PageRoundButton::ALL
                .map(|button| (button.label.into(), Control::Front(button.control))),
        );
    }

    fn page_round_state(&self) -> String {
        let Some(exploration) = &self.exploration else {
            return "Starting: the reviewer prepares the round.".into();
        };
        if self.durable.error.is_some() || self.durable.historical {
            return self.status.clone();
        }
        if let Some(line) = self.finishing_line() {
            return line;
        }
        match self.progress {
            Progress::Retryable => format!("Interrupted: {}", self.status),
            Progress::Waiting | Progress::Capturing | Progress::DiscardingCapture => {
                "The agent is working.".into()
            }
            Progress::Ready if self.implementation_not_started() => {
                "Concluded: the agent did not start on the implementation request. Look at the agent's pane, then Retry in the pane.".into()
            }
            Progress::Ready if exploration.conclusion.is_some() => {
                "Concluded: the conclusion is on the page.".into()
            }
            Progress::Ready if exploration.questions.is_empty() => "The agent is working.".into(),
            Progress::Ready => "A question waits for your answer.".into(),
        }
    }
}
