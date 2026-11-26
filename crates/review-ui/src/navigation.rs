use component_core::{Component, ComponentSubscriptions};
use ui_events::{ReviewNavigation, ReviewNavigationChanged, ReviewPane, ReviewPaneFocusRequested};

use crate::Action;
use crate::layout::{ScreenLayout, Viewport};

pub(crate) struct Navigation {
    mode: ReviewNavigation,
    focus: ReviewPane,
    remembered: FocusMemory,
    modals: OpenModals,
    layout: ScreenLayout,
}

#[derive(Clone, Copy)]
struct FocusMemory {
    files: ReviewPane,
    threads: ReviewPane,
    explore: ReviewPane,
}

impl FocusMemory {
    fn pane_mut(&mut self, mode: ReviewNavigation) -> &mut ReviewPane {
        match mode {
            ReviewNavigation::Files => &mut self.files,
            ReviewNavigation::Threads => &mut self.threads,
            ReviewNavigation::Explore => &mut self.explore,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PaneComponent {
    Files,
    Threads,
    Explore,
    Diff,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Modal {
    Revision,
    Overlay,
    Locations,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct OpenModals {
    pub(crate) revision: bool,
    pub(crate) overlay: bool,
    pub(crate) locations: bool,
}

impl Modal {
    const PRECEDENCE: [Self; 3] = [Self::Revision, Self::Overlay, Self::Locations];
}

impl OpenModals {
    pub(crate) fn is_open(self, modal: Modal) -> bool {
        match modal {
            Modal::Revision => self.revision,
            Modal::Overlay => self.overlay,
            Modal::Locations => self.locations,
        }
    }

    fn top(self) -> Option<Modal> {
        Modal::PRECEDENCE
            .into_iter()
            .find(|modal| self.is_open(*modal))
    }
}

impl Navigation {
    pub(crate) fn new(viewport: Viewport) -> Self {
        let mode = ReviewNavigation::Files;
        let focus = ReviewPane::Navigation;
        Self {
            mode,
            focus,
            remembered: FocusMemory {
                files: focus,
                threads: focus,
                explore: focus,
            },
            modals: OpenModals::default(),
            layout: ScreenLayout::new(viewport, mode, focus, false),
        }
    }

    pub(crate) fn mode(&self) -> ReviewNavigation {
        self.mode
    }

    pub(crate) fn focus(&self) -> ReviewPane {
        self.focus
    }

    pub(crate) fn layout(&self) -> ScreenLayout {
        self.layout
    }

    pub(crate) fn apply(&mut self, requests: NavigationRequests) {
        if let Some(mode) = requests.mode
            && mode != self.mode
        {
            *self.remembered.pane_mut(self.mode) = self.focus;
            self.focus = *self.remembered.pane_mut(mode);
            self.mode = mode;
        }
        if let Some(focus) = requests.focus {
            self.focus = focus;
        }
    }

    pub(crate) fn focus_pane(&mut self, focus: ReviewPane) {
        self.focus = focus;
    }

    pub(crate) fn toggle_focus(&mut self) -> ReviewPane {
        self.focus = match self.focus {
            ReviewPane::Navigation => ReviewPane::Detail,
            ReviewPane::Detail => ReviewPane::Navigation,
        };
        self.focus
    }

    pub(crate) fn focused_pane(&self) -> PaneComponent {
        match self.focus {
            ReviewPane::Navigation => self.navigation_pane(),
            ReviewPane::Detail => PaneComponent::Diff,
        }
    }

    pub(crate) fn navigation_pane(&self) -> PaneComponent {
        match self.mode {
            ReviewNavigation::Files => PaneComponent::Files,
            ReviewNavigation::Threads => PaneComponent::Threads,
            ReviewNavigation::Explore => PaneComponent::Explore,
        }
    }

    pub(crate) fn focus_is_enclosed(&self) -> bool {
        self.mode == ReviewNavigation::Explore && self.focus == ReviewPane::Detail
    }

    pub(crate) fn set_modals(&mut self, modals: OpenModals) {
        self.modals = modals;
    }

    pub(crate) fn modals(&self) -> OpenModals {
        self.modals
    }

    pub(crate) fn modal(&self) -> Option<Modal> {
        self.modals.top()
    }

    pub(crate) fn relayout(&mut self, viewport: Viewport) {
        self.layout = ScreenLayout::new(viewport, self.mode, self.focus, self.modals.locations);
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct NavigationRequests {
    mode: Option<ReviewNavigation>,
    focus: Option<ReviewPane>,
}

impl NavigationRequests {
    pub(crate) fn take(&mut self) -> Self {
        std::mem::take(self)
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn mode_requested(&mut self, event: &ReviewNavigationChanged) {
        self.mode = Some(event.0);
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn focus_requested(&mut self, event: &ReviewPaneFocusRequested) {
        self.focus = Some(event.0);
    }
}

impl Component<Action> for NavigationRequests {
    fn register_subscriptions(subscriptions: &mut ComponentSubscriptions<'_, Self, Action>) {
        subscriptions.subscribe(Self::mode_requested);
        subscriptions.subscribe(Self::focus_requested);
    }
}

#[cfg(test)]
#[path = "navigation.tests.rs"]
mod tests;
