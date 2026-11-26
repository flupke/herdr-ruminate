use review_explore_page_tunnel::TunnelState;
use toasts::ToastKind;
use ui_actions::{Action, TerminalAction};
use ui_events::ToastRequested;
use ui_qr_code::QrCode;
use ui_theme::Palette;

use super::{Control, ExploreComponent, flow::ConversationLayout};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SharedLink {
    Local,
    Network,
    Tunnel,
}

pub(super) struct PageLink {
    url: String,
    code: Option<QrCode>,
}

impl PageLink {
    fn new(url: String) -> Self {
        Self {
            code: QrCode::new(&url),
            url,
        }
    }

    fn lay_out(&self, layout: &mut ConversationLayout, palette: Palette, link: SharedLink) {
        layout.link(self.url.clone(), palette.text, Control::CopyLink(link));
        if let Some(code) = self
            .code
            .as_ref()
            .filter(|code| code.width() <= layout.area.width)
        {
            layout.picture(code.text(), code.height(), Some(Control::CopyLink(link)));
        }
    }
}

pub(super) enum NetworkPage {
    Shared(PageLink),
    NotShared(String),
}

#[derive(Default)]
pub(super) struct TunnelView {
    state: TunnelState,
    link: Option<PageLink>,
}

impl TunnelView {
    pub(super) fn new(state: TunnelState) -> Self {
        let link = match &state {
            TunnelState::Open { url } => Some(PageLink::new(url.clone())),
            _ => None,
        };
        Self { state, link }
    }

    pub(super) fn state(&self) -> &TunnelState {
        &self.state
    }
}

impl ExploreComponent {
    pub(super) fn page_shared(&mut self, event: &ui_events::ExplorePageShared) {
        self.network_page = Some(NetworkPage::Shared(PageLink::new(event.0.clone())));
    }

    pub(super) fn page_not_shared(&mut self, event: &ui_events::ExplorePageNotShared) {
        self.network_page = Some(NetworkPage::NotShared(event.0.clone()));
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub(super) fn page_off_network(&mut self, _: &ui_events::ExplorePageOffNetwork) {
        self.network_page = None;
    }

    pub(super) fn lay_out_network_page(&self, layout: &mut ConversationLayout, palette: Palette) {
        let Some(page) = &self.network_page else {
            return;
        };
        layout.gap();
        let link = match page {
            NetworkPage::Shared(link) => link,
            NetworkPage::NotShared(reason) => {
                layout.text(
                    format!("The page is not shared on the network: {reason}"),
                    palette.dim,
                    None,
                );
                return;
            }
        };
        let invitation = if self.exploration.is_some() {
            "Open this round on a phone on the same network:"
        } else {
            "Start a round from a phone on the same network:"
        };
        layout.text(invitation, palette.dim, None);
        link.lay_out(layout, palette, SharedLink::Network);
    }

    pub(super) fn copy_link(&self, link: SharedLink) -> Vec<Action> {
        let (url, copied) = match link {
            SharedLink::Local => match self
                .unopened_page
                .as_ref()
                .and_then(|page| page.url.as_ref())
            {
                Some(url) => (url, "the page's address"),
                None => return Vec::new(),
            },
            SharedLink::Network => match &self.network_page {
                Some(NetworkPage::Shared(page_link)) => (&page_link.url, "the page's address"),
                _ => return Vec::new(),
            },
            SharedLink::Tunnel => match &self.page_settings.tunnel().link {
                Some(page_link) => (&page_link.url, "the tunnel's link"),
                None => return Vec::new(),
            },
        };
        self.events.publish(ToastRequested {
            text: format!("Copied {copied} to the clipboard"),
            kind: ToastKind::Info,
        });
        vec![Action::Terminal(TerminalAction::CopyToClipboard(
            url.clone(),
        ))]
    }

    pub(super) fn lay_out_tunnel(&self, layout: &mut ConversationLayout, palette: Palette) {
        let tunnel = self.page_settings.tunnel();
        let (line, color) = match tunnel.state() {
            TunnelState::Off => return,
            TunnelState::Opening => ("Opening a tunnel with cloudflared…".into(), palette.dim),
            TunnelState::Failed(reason) => (
                format!("Cannot share the round over a tunnel: {reason}"),
                palette.warning,
            ),
            TunnelState::Open { .. } => (
                "Share this round: anyone with this link can use it as you, Implement included:"
                    .into(),
                palette.warning,
            ),
        };
        layout.gap();
        layout.text(line, color, None);
        if let Some(link) = &tunnel.link {
            link.lay_out(layout, palette, SharedLink::Tunnel);
        }
    }
}
