use review_explore_round_settings::{ExploreRoundSettings, RunAhead, WritingStyle};
use ui_actions::{Action, SettingsAction};
use ui_events::{ExploreRoundSettingsLoaded, ExploreRunAheadUnavailable};
use ui_shortcuts::ExploreSettingShortcut;
use ui_theme::Palette;

use super::flow::ConversationLayout;
use super::{Control, ExploreComponent};

#[derive(Default)]
pub(super) struct RoundSettings {
    saved: ExploreRoundSettings,
    run_ahead_unavailable: Option<String>,
}

impl RoundSettings {
    pub(super) fn writing(&self) -> WritingStyle {
        self.saved.writing
    }

    pub(super) fn cycle_writing(&mut self) -> Vec<Action> {
        self.saved.writing.cycle();
        vec![Action::Settings(SettingsAction::SaveExploreWritingStyle(
            self.saved.writing,
        ))]
    }

    pub(super) fn cycle_run_ahead(&mut self) -> Vec<Action> {
        self.saved.run_ahead.cycle();
        vec![Action::Settings(SettingsAction::SaveExploreRunAhead(
            self.saved.run_ahead,
        ))]
    }

    fn lay_out(&self, layout: &mut ConversationLayout, palette: Palette) {
        let style = match self.saved.writing {
            WritingStyle::Plain => "plain",
            WritingStyle::SimplifiedTechnicalEnglish => "Simplified Technical English",
            WritingStyle::Concise => "concise",
        };
        let run_ahead = match self.saved.run_ahead {
            RunAhead::Off => "off",
            RunAhead::Recommended => "the recommended choice",
            RunAhead::Every => "every choice",
        };
        layout.gap();
        layout.text("Explore round settings:", palette.dim, None);
        layout.controls([
            (
                format!("Writing style of the next round: {style}"),
                Control::Setting(ExploreSettingShortcut::WritingStyle),
            ),
            (
                format!("Run ahead (experimental): {run_ahead}"),
                Control::Setting(ExploreSettingShortcut::RunAhead),
            ),
        ]);
        if let Some(why) = &self.run_ahead_unavailable {
            layout.text(
                format!("Run-ahead is not available for this agent: {why}."),
                palette.dim,
                None,
            );
        }
    }
}

impl ExploreComponent {
    pub(super) fn round_settings_loaded(&mut self, event: &ExploreRoundSettingsLoaded) {
        self.round_settings.saved = event.0;
    }

    pub(super) fn run_ahead_unavailable(&mut self, event: &ExploreRunAheadUnavailable) {
        self.round_settings
            .run_ahead_unavailable
            .clone_from(&event.0);
    }

    pub(super) fn lay_out_round_settings(&self, layout: &mut ConversationLayout, palette: Palette) {
        self.round_settings.lay_out(layout, palette);
    }
}
