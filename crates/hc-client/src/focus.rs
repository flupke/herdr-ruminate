//! The terminal the user focuses in herdr-app, followed as Herdr's `pane.focused` events tell
//! it: herdr-core keeps no focus, so the reviewer reads herdr-app's view and layout.

use herdr_app_focus::{AppHome, Layout};
use herdr_client::protocol::PaneId;

/// The terminal herdr-app's screen focuses, as last told.
#[derive(Debug)]
pub(crate) struct Focus {
    home: AppHome,
    layout: Layout,
    /// The terminal last told; `None` while the focus is on a mod, or unknown, so that the next
    /// focused terminal is told even when it was the one before.
    told: Option<PaneId>,
}

impl Focus {
    /// The focus of the screen whose home is `home`, in `layout`, of which nothing is told yet.
    pub(crate) fn new(home: AppHome, layout: Layout) -> Self {
        Self {
            home,
            layout,
            told: None,
        }
    }

    /// The terminal the user focused when herdr-app wrote its view or its layout last, if it is
    /// not the one told already.
    pub(crate) fn changed(&mut self) -> Option<PaneId> {
        let focused = self.home.focused_terminal(&self.layout).map(PaneId);
        if focused == self.told {
            return None;
        }
        self.told.clone_from(&focused);
        focused
    }

    /// herdr-app saved `layout`: a pane focused before its layout was saved may show a terminal
    /// now. The terminal the user focuses then, if it is not the one told already.
    pub(crate) fn layout_changed(&mut self, layout: Layout) -> Option<PaneId> {
        self.layout = layout;
        self.changed()
    }
}

#[cfg(test)]
#[path = "focus.tests.rs"]
mod tests;
