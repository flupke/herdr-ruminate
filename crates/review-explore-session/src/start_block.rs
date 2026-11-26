use review_explore::StartBlock;
use review_repository::repository::{PollResult, Snapshot};
use review_state::ReviewState;

use crate::ExploreSession;

impl ExploreSession {
    pub fn marks_changed(&mut self, snapshot: &Snapshot, states: &[ReviewState]) {
        self.read_marks(snapshot);
        let block = StartBlock::of(states.iter().map(|state| state.status.needs_review()));
        if block != self.start_block {
            self.start_block = block;
            let _ = self.events.send(ui_events::ExploreStartBlock(block));
        }
        self.page.block_starts(block);
        self.page.tally(self.mark_tally());
    }

    pub(crate) fn refresh_start_block(&mut self, snapshot: &Snapshot) -> Option<StartBlock> {
        let states = self.tracker.statuses(snapshot).ok()?;
        self.marks_changed(snapshot, &states);
        self.start_block
    }

    pub(crate) fn kickoff_block(&mut self) -> Option<StartBlock> {
        let Ok(PollResult::Complete(snapshot)) = self.repository.poll() else {
            return None;
        };
        self.refresh_start_block(&snapshot)
    }
}
