use herdr_app_focus::{AppHome, Layout};
use herdr_client::protocol::PaneId;

#[derive(Debug)]
pub(crate) struct Focus {
    home: AppHome,
    layout: Layout,
    told: Option<PaneId>,
}

impl Focus {
    pub(crate) fn new(home: AppHome, layout: Layout) -> Self {
        Self {
            home,
            layout,
            told: None,
        }
    }

    pub(crate) fn changed(&mut self) -> Option<PaneId> {
        let focused = self.home.focused_terminal(&self.layout).map(PaneId);
        if focused == self.told {
            return None;
        }
        self.told.clone_from(&focused);
        focused
    }

    pub(crate) fn layout_changed(&mut self, layout: Layout) -> Option<PaneId> {
        self.layout = layout;
        self.changed()
    }
}

#[cfg(test)]
#[path = "focus.tests.rs"]
mod tests;
