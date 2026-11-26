use review_explore::{ChangeAuthor, RoundFront, StartBlock};
use ui_controls::KeyHint;
use ui_shortcuts::{
    ExploreSettingShortcut, ExploreShortcut, ExploreStartShortcut, ExploreTurnShortcut,
    ShortcutSubscription,
};
use ui_theme::Palette;

use super::flow::ConversationLayout;
use super::{Control, ExploreComponent};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RoundStart {
    pub(super) challenger: bool,
    pub(super) front: RoundFront,
}

#[derive(Clone, Copy)]
pub(super) struct StartButton {
    pub(super) start: RoundStart,
    pub(super) shortcut: ExploreStartShortcut,
    pub(super) label: &'static str,
    pub(super) hint: &'static str,
}

impl StartButton {
    pub(super) const START: Self = Self::new(
        false,
        RoundFront::Page,
        ExploreStartShortcut::Start,
        "Start",
        "start",
    );
    pub(super) const START_WITH_CHALLENGER: Self = Self::new(
        true,
        RoundFront::Page,
        ExploreStartShortcut::StartWithChallenger,
        "Start with Challenger",
        "with Challenger",
    );
    pub(super) const START_IN_PANE: Self = Self::new(
        false,
        RoundFront::Pane,
        ExploreStartShortcut::StartInPane,
        "Start in the pane",
        "in the pane",
    );
    pub(super) const START_IN_PANE_WITH_CHALLENGER: Self = Self::new(
        true,
        RoundFront::Pane,
        ExploreStartShortcut::StartInPaneWithChallenger,
        "Start in the pane with Challenger",
        "in the pane with Challenger",
    );

    pub(super) const ALL: [Self; 4] = [
        Self::START,
        Self::START_WITH_CHALLENGER,
        Self::START_IN_PANE,
        Self::START_IN_PANE_WITH_CHALLENGER,
    ];

    const fn new(
        challenger: bool,
        front: RoundFront,
        shortcut: ExploreStartShortcut,
        label: &'static str,
        hint: &'static str,
    ) -> Self {
        Self {
            start: RoundStart { challenger, front },
            shortcut,
            label,
            hint,
        }
    }

    pub(super) fn of(shortcut: ExploreStartShortcut) -> Self {
        match shortcut {
            ExploreStartShortcut::Start => Self::START,
            ExploreStartShortcut::StartWithChallenger => Self::START_WITH_CHALLENGER,
            ExploreStartShortcut::StartInPane => Self::START_IN_PANE,
            ExploreStartShortcut::StartInPaneWithChallenger => Self::START_IN_PANE_WITH_CHALLENGER,
        }
    }
}

impl RoundStart {
    pub(super) fn is_main(self) -> bool {
        self == StartButton::START.start
    }
}

#[derive(Clone, Copy)]
pub(super) struct PageRoundButton {
    pub(super) label: &'static str,
    pub(super) control: FrontControl,
    pub(super) shortcut: ExploreTurnShortcut,
    pub(super) hint: &'static str,
}

impl PageRoundButton {
    pub(super) const ALL: [Self; 2] = [
        Self {
            label: "Open the Explore page",
            control: FrontControl::OpenPage,
            shortcut: ExploreTurnShortcut::OpenPage,
            hint: "open the page",
        },
        Self {
            label: "Continue in the pane",
            control: FrontControl::ContinueInPane,
            shortcut: ExploreTurnShortcut::ContinueInPane,
            hint: "continue in the pane",
        },
    ];

    pub(super) fn of(shortcut: ExploreTurnShortcut) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|button| button.shortcut == shortcut)
    }

    pub(super) fn hints() -> Vec<KeyHint> {
        Self::ALL
            .into_iter()
            .filter_map(|button| key_hint(button.shortcut, button.hint))
            .collect()
    }
}

impl StartButton {
    pub(super) fn hints(block: Option<StartBlock>) -> Vec<KeyHint> {
        if block.is_some() {
            return Vec::new();
        }
        key_hint(
            ExploreTurnShortcut::Setting(ExploreSettingShortcut::Whose),
            "whose change",
        )
        .into_iter()
        .chain(Self::ALL.into_iter().filter_map(|button| {
            key_hint(ExploreTurnShortcut::Start(button.shortcut), button.hint)
        }))
        .collect()
    }

    pub(super) fn lay_out(
        block: Option<StartBlock>,
        author: ChangeAuthor,
        layout: &mut ConversationLayout,
        palette: Palette,
    ) {
        let Some(block) = block else {
            let whose = match author {
                ChangeAuthor::Reviewer => "mine",
                ChangeAuthor::Other => "someone else's (learn it, then review it for its author)",
            };
            layout.controls([(
                format!("Whose change: {whose}"),
                Control::Setting(ExploreSettingShortcut::Whose),
            )]);
            layout.gap();
            layout.controls(Self::ALL.map(|button| {
                (
                    button.label.into(),
                    Control::Front(FrontControl::Start(button.start)),
                )
            }));
            return;
        };
        layout.inactive_controls(Self::ALL.map(|button| button.label.into()));
        layout.gap();
        layout.text(block.reason(), palette.dim, None);
    }
}

impl ExploreComponent {
    pub(super) fn turn_over_whose(&mut self) {
        if !self.offers_starts() || self.start_block.is_some() {
            return;
        }
        self.author.toggle();
    }
}

pub(super) fn key_hint(shortcut: ExploreTurnShortcut, hint: &str) -> Option<KeyHint> {
    let key = ExploreShortcut::Turn(shortcut).keys().next()?;
    Some(KeyHint::new(key.label(), hint))
}

#[derive(Clone, Copy, Debug)]
pub(super) enum FrontControl {
    Start(RoundStart),
    OpenPage,
    ContinueInPane,
}
