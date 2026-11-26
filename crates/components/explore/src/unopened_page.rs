use ui_events::ExplorePageNotOpened;
use ui_theme::Palette;

use super::{Control, ExploreComponent, flow::ConversationLayout, network_page::SharedLink};

impl ExploreComponent {
    pub(super) fn page_not_opened(&mut self, event: &ExplorePageNotOpened) {
        self.unopened_page = Some(event.0.clone());
    }

    pub(super) fn lay_out_unopened_page(&self, layout: &mut ConversationLayout, palette: Palette) {
        let Some(page) = &self.unopened_page else {
            return;
        };
        layout.gap();
        layout.text(page.to_string(), palette.warning, None);
        if let Some(url) = &page.url {
            layout.text("Open it in a browser on this machine:", palette.dim, None);
            layout.link(
                url.clone(),
                palette.text,
                Control::CopyLink(SharedLink::Local),
            );
        }
    }
}
