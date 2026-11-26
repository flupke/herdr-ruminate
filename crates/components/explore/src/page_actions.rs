use super::{ExploreComponent, Progress};
use ui_events::{ExplorePageReset, ExplorePageStopped, ExplorePosted};

impl ExploreComponent {
    pub(super) fn page_stopped(&mut self, event: &ExplorePageStopped) {
        if self.awaiting_page_start() {
            self.show_start_screen();
            return;
        }
        let stopped = self.stopped_status(
            "Stopped waiting on the Explore page. Your answer and text remain available; Retry \
             sends them again.",
        );
        let Some(exploration) = &mut self.exploration else {
            return;
        };
        match &event.round {
            None if !self.durable.persisted => self.show_start_screen(),
            Some(round) if exploration.instance == *round && self.progress == Progress::Waiting => {
                exploration.cancel();
                self.durable.posting = None;
                self.progress = Progress::Retryable;
                self.status = stopped.into();
            }
            _ => {}
        }
    }

    pub(super) fn page_reset(&mut self, event: &ExplorePageReset) {
        if self
            .exploration
            .as_ref()
            .is_some_and(|exploration| exploration.instance == event.round)
        {
            self.show_start_screen();
        }
    }

    pub(super) fn retried_elsewhere(&self, event: &ExplorePosted) -> bool {
        self.progress == Progress::Retryable
            && self.durable.posting.is_none()
            && self
                .exploration
                .as_ref()
                .and_then(|exploration| exploration.retry_request())
                .is_some_and(|retry| retry.request == event.request.request)
    }
}
