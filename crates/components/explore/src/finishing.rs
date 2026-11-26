use std::time::{Duration, Instant};

use ui_events::{ExploreTurnFinishing, ToastExpirationTick};

use super::{ExploreComponent, Progress};

pub(super) struct FinishingTurn {
    request: String,
    started: Instant,
    seconds: u64,
}

impl FinishingTurn {
    fn seconds_at(&self, now: Instant) -> u64 {
        now.saturating_duration_since(self.started).as_secs()
    }
}

impl ExploreComponent {
    pub(super) fn turn_finishing(&mut self, event: &ExploreTurnFinishing) {
        self.finishing_since(event, review_explore::now_ms(), Instant::now());
    }

    pub(super) fn finishing_since(
        &mut self,
        event: &ExploreTurnFinishing,
        now_ms: u64,
        now: Instant,
    ) {
        if self
            .exploration
            .as_ref()
            .is_none_or(|exploration| exploration.instance != event.round)
        {
            return;
        }
        let elapsed = Duration::from_millis(now_ms.saturating_sub(event.started_at_ms));
        let started = now.checked_sub(elapsed).unwrap_or(now);
        self.finishing = Some(FinishingTurn {
            request: event.request.clone(),
            started,
            seconds: elapsed.as_secs(),
        });
    }

    pub(super) fn turn_finished(&mut self, request: &str) {
        self.finishing
            .take_if(|finishing| finishing.request == request);
    }

    pub(super) fn stopped_status<'a>(&self, otherwise: &'a str) -> &'a str {
        if self.shown_finishing().is_some() {
            "Stopped waiting for the turn the agent started while you were thinking: your answer \
             goes to the agent."
        } else {
            otherwise
        }
    }

    fn shown_finishing(&self) -> Option<&FinishingTurn> {
        let finishing = self.finishing.as_ref()?;
        let pending = self.exploration.as_ref()?.pending_request()?;
        (self.progress == Progress::Waiting && pending.request == finishing.request)
            .then_some(finishing)
    }

    pub(super) fn finishing_line(&self) -> Option<String> {
        let seconds = self.shown_finishing()?.seconds;
        Some(format!(
            "The agent is finishing a turn it started while you were thinking · {}:{:02}",
            seconds / 60,
            seconds % 60
        ))
    }

    pub fn finishing_ticks_between(&self, previous: Instant, now: Instant) -> bool {
        self.shown_finishing()
            .is_some_and(|finishing| finishing.seconds_at(previous) != finishing.seconds_at(now))
    }

    pub(super) fn finishing_tick(&mut self, event: &ToastExpirationTick) {
        if let Some(finishing) = &mut self.finishing {
            finishing.seconds = finishing.seconds_at(event.now);
        }
    }
}
