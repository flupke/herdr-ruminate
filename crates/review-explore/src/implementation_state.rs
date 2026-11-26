use serde::Serialize;
use ts_rs::TS;

use crate::DispatchState;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, TS)]
#[serde(tag = "kind", content = "reason", rename_all = "snake_case")]
pub enum ImplementationState {
    Sending,
    Sent,
    Paused,
    Unknown,
    NotStarted,
    NotSent(String),
    Cancelled,
}

impl ImplementationState {
    pub fn of(state: &DispatchState, sending: bool) -> Self {
        match state {
            DispatchState::Queued | DispatchState::Attempting if sending => Self::Sending,
            DispatchState::Queued => Self::Paused,
            DispatchState::Attempting | DispatchState::Unknown => Self::Unknown,
            DispatchState::Delivered => Self::Sent,
            DispatchState::NotStarted => Self::NotStarted,
            DispatchState::NotSent(reason) => Self::NotSent(reason.clone()),
            DispatchState::Cancelled => Self::Cancelled,
        }
    }

    pub fn allows_another(&self) -> bool {
        matches!(
            self,
            Self::Paused | Self::Unknown | Self::NotSent(_) | Self::Cancelled
        )
    }
}
