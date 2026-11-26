use comment_editor::{CommentEditor, KeymapSetting};
use review_explore_page_settings::{
    ExplorePageSetting, ExplorePageSettings, NetworkAccess, PaneStarts,
};
use review_explore_page_tunnel::TunnelState;
use ui_actions::{Action, ExplorePageAction, SettingsAction};
use ui_events::{ExplorePageSettingsLoaded, ExplorePageTunnel};
use ui_shortcuts::{
    ExploreCommand, ExploreGlobalShortcut, ExplorePageSettingShortcut, ExploreSettingShortcut,
    ExploreShortcut,
};
use ui_theme::Palette;

use super::flow::{Content, ConversationLayout};
use super::input::ExploreKey;
use super::network_page::TunnelView;
use super::{Control, ExploreComponent};

#[derive(Default)]
pub(super) struct PageSettings {
    saved: ExplorePageSettings,
    field: Option<FieldEdit>,
    tunnel: TunnelView,
}

struct FieldEdit {
    field: TextField,
    editor: CommentEditor,
    error: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TextField {
    Interface,
    FirstPort,
}

impl TextField {
    fn title(self) -> &'static str {
        match self {
            Self::Interface => {
                "Network interface · empty for the default route · Enter save · Tab cancel"
            }
            Self::FirstPort => "First port · Enter save · Tab cancel",
        }
    }

    fn text(self, network: &NetworkAccess) -> String {
        match self {
            Self::Interface => network.interface.clone().unwrap_or_default(),
            Self::FirstPort => network.first_port.to_string(),
        }
    }

    fn setting(self, text: &str) -> Result<ExplorePageSetting, String> {
        match self {
            Self::Interface => Ok(ExplorePageSetting::interface(text)),
            Self::FirstPort => ExplorePageSetting::first_port(text),
        }
    }
}

impl PageSettings {
    pub(super) fn pane_starts(&self) -> PaneStarts {
        self.saved.pane_starts
    }

    pub(super) fn tunnel(&self) -> &TunnelView {
        &self.tunnel
    }

    pub(super) fn editing(&self) -> bool {
        self.field.is_some()
    }

    pub(super) fn editor(&self) -> Option<(&CommentEditor, &'static str)> {
        self.field
            .as_ref()
            .map(|field| (&field.editor, field.field.title()))
    }

    pub(super) fn change(
        &mut self,
        setting: ExplorePageSettingShortcut,
        keymap: &KeymapSetting,
    ) -> Vec<Action> {
        let field = match setting {
            ExplorePageSettingShortcut::PaneStarts => {
                let mut starts = self.saved.pane_starts;
                starts.toggle();
                return self.save(ExplorePageSetting::PaneStarts(starts));
            }
            ExplorePageSettingShortcut::Network => {
                let enabled = !self.saved.network.enabled;
                return self.save(ExplorePageSetting::NetworkEnabled(enabled));
            }
            ExplorePageSettingShortcut::Tunnel => return self.toggle_tunnel(),
            ExplorePageSettingShortcut::Interface => TextField::Interface,
            ExplorePageSettingShortcut::FirstPort => TextField::FirstPort,
        };
        self.field = Some(FieldEdit {
            field,
            editor: CommentEditor::single_line(&field.text(&self.saved.network), keymap),
            error: None,
        });
        Vec::new()
    }

    fn toggle_tunnel(&mut self) -> Vec<Action> {
        let (state, action) = if self.tunnel.state().is_on() {
            (TunnelState::Off, ExplorePageAction::CloseTunnel)
        } else {
            (TunnelState::Opening, ExplorePageAction::OpenTunnel)
        };
        self.tunnel = TunnelView::new(state);
        vec![Action::ExplorePage(action)]
    }

    fn save_field(&mut self) -> Vec<Action> {
        let Some(edit) = &mut self.field else {
            return Vec::new();
        };
        match edit.field.setting(&edit.editor.text()) {
            Ok(setting) => {
                self.field = None;
                self.save(setting)
            }
            Err(error) => {
                edit.error = Some(error);
                Vec::new()
            }
        }
    }

    fn save(&mut self, setting: ExplorePageSetting) -> Vec<Action> {
        self.saved.set(setting.clone());
        vec![Action::Settings(SettingsAction::SaveExplorePage(setting))]
    }

    fn close_field(&mut self) {
        self.field = None;
    }

    fn key(&mut self, input: ExploreKey) -> Option<Vec<Action>> {
        let edit = self.field.as_mut()?;
        match input.command {
            Some(ExploreCommand::Explore(ExploreShortcut::Confirm)) => {
                return Some(self.save_field());
            }
            Some(ExploreCommand::Global(ExploreGlobalShortcut::CycleFocus)) => self.close_field(),
            _ => edit.editor.input(input.key),
        }
        Some(Vec::new())
    }

    fn paste(&mut self, text: &str) -> bool {
        let Some(edit) = &mut self.field else {
            return false;
        };
        edit.editor.paste(text);
        true
    }

    fn lay_out(&self, layout: &mut ConversationLayout, palette: Palette) {
        let network = &self.saved.network;
        let on = |value| if value { "on" } else { "off" };
        layout.gap();
        layout.text("Explore page settings:", palette.dim, None);
        layout.controls([
            (
                format!(
                    "Open the page on Start: {}",
                    on(self.saved.pane_starts == PaneStarts::OnPage)
                ),
                Control::Setting(ExploreSettingShortcut::Page(
                    ExplorePageSettingShortcut::PaneStarts,
                )),
            ),
            (
                format!("Serve on the network: {}", on(network.enabled)),
                Control::Setting(ExploreSettingShortcut::Page(
                    ExplorePageSettingShortcut::Network,
                )),
            ),
            (
                format!(
                    "Interface: {}",
                    network.interface.as_deref().unwrap_or("default route")
                ),
                Control::Setting(ExploreSettingShortcut::Page(
                    ExplorePageSettingShortcut::Interface,
                )),
            ),
            (
                format!("First port: {}", network.first_port),
                Control::Setting(ExploreSettingShortcut::Page(
                    ExplorePageSettingShortcut::FirstPort,
                )),
            ),
            (
                format!(
                    "Share over a tunnel: {}",
                    match self.tunnel.state() {
                        TunnelState::Off | TunnelState::Failed(_) => "off",
                        TunnelState::Opening => "opening",
                        TunnelState::Open { .. } => "on",
                    }
                ),
                Control::Setting(ExploreSettingShortcut::Page(
                    ExplorePageSettingShortcut::Tunnel,
                )),
            ),
        ]);
        let Some(edit) = &self.field else {
            return;
        };
        layout.push(Content::SettingEditor, 3);
        if let Some(error) = &edit.error {
            layout.text(error.clone(), palette.warning, None);
        }
    }
}

impl ExploreComponent {
    pub(super) fn page_settings_loaded(&mut self, event: &ExplorePageSettingsLoaded) {
        self.page_settings.saved = event.0.clone();
    }

    pub(super) fn tunnel_reported(&mut self, event: &ExplorePageTunnel) {
        self.page_settings.tunnel = TunnelView::new(event.0.clone());
    }

    pub(super) fn leave_page_setting(&mut self) {
        self.page_settings.close_field();
    }

    pub(super) fn page_setting_key(&mut self, input: ExploreKey) -> Option<Vec<Action>> {
        self.page_settings.key(input)
    }

    pub(super) fn paste_page_setting(&mut self, text: &str) -> bool {
        self.page_settings.paste(text)
    }

    pub(super) fn lay_out_page_settings(&self, layout: &mut ConversationLayout, palette: Palette) {
        self.page_settings.lay_out(layout, palette);
    }
}
