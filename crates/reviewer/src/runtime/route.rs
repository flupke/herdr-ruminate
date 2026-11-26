use std::time::Instant;

use component_core::EventEnvelope;
use herdr_client::protocol::{HerdrEvent, PaneId};
use review_ui::UserInput;

pub(super) struct WorkerStopped;
pub(super) struct StopRequested;
pub(super) struct RepositoryRefreshDue;
pub(super) struct ApplicationTick(pub(super) Instant);
pub(super) struct TerminalFailed(pub(super) String);
pub(super) struct TerminalFocused;
pub(super) struct InputAcknowledged(pub(super) u64);

#[derive(Debug)]
pub(super) enum Route<'a> {
    Fail(String),
    Stop,
    AgentFocused(&'a PaneId),
    Herdr(&'a HerdrEvent),
    RefreshDue,
    Tick(Instant),
    Input(&'a UserInput),
    Lsp(&'a review_lsp::Event),
    TerminalFocused,
    Acknowledged(u64),
    Application,
}

impl<'a> Route<'a> {
    pub(super) fn of(event: &'a EventEnvelope) -> Self {
        if let Some(event) = event.downcast_ref::<HerdrEvent>() {
            return match event {
                HerdrEvent::PaneFocused(pane_id) => Self::AgentFocused(pane_id),
                event @ (HerdrEvent::AgentDetected { .. } | HerdrEvent::AgentSessionChanged(_)) => {
                    Self::Herdr(event)
                }
            };
        }
        if let Some(input) = event.downcast_ref::<UserInput>() {
            return Self::Input(input);
        }
        if let Some(ApplicationTick(now)) = event.downcast_ref::<ApplicationTick>() {
            return Self::Tick(*now);
        }
        if let Some(event) = event.downcast_ref::<review_lsp::Event>() {
            return Self::Lsp(event);
        }
        Self::runtime_event(event).unwrap_or(Self::Application)
    }

    fn runtime_event(event: &EventEnvelope) -> Option<Self> {
        if event.downcast_ref::<WorkerStopped>().is_some() {
            return Some(Self::Fail("review worker stopped unexpectedly".into()));
        }
        if let Some(TerminalFailed(message)) = event.downcast_ref::<TerminalFailed>() {
            return Some(Self::Fail(format!(
                "could not read terminal input: {message}"
            )));
        }
        if event.downcast_ref::<StopRequested>().is_some() {
            return Some(Self::Stop);
        }
        if event.downcast_ref::<RepositoryRefreshDue>().is_some() {
            return Some(Self::RefreshDue);
        }
        if let Some(InputAcknowledged(id)) = event.downcast_ref::<InputAcknowledged>() {
            return Some(Self::Acknowledged(*id));
        }
        event
            .downcast_ref::<TerminalFocused>()
            .map(|_| Self::TerminalFocused)
    }

    pub(super) fn needs_frame(&self) -> bool {
        !matches!(self, Self::Herdr(_))
    }

    pub(super) fn resets_cursor(&self) -> bool {
        matches!(
            self,
            Self::Input(_) | Self::TerminalFocused | Self::AgentFocused(_)
        )
    }

    // herdr-core drops the rows above the cursor when a terminal shrinks and gives blank ones back
    // when it grows: a pane can keep its size yet lose the top of its last frame.
    pub(super) fn repaints(&self) -> bool {
        matches!(self, Self::Input(UserInput::Resize { .. }))
    }
}

#[cfg(test)]
#[path = "route.tests.rs"]
mod tests;
