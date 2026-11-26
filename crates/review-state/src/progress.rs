use review_repository::repository::DiffStatistics;

use crate::{ReviewState, ReviewStatus};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReviewProgress {
    reviewed_lines: u64,
    total_lines: u64,
}

impl ReviewProgress {
    fn add(&mut self, statistics: DiffStatistics, state: &ReviewState) {
        let total = changed_lines(statistics);
        let remaining = match state.status {
            ReviewStatus::Reviewed => 0,
            ReviewStatus::Unreviewed => total,
            ReviewStatus::ChangedSinceReview | ReviewStatus::PartiallyReviewed => {
                changed_lines(state.current_diff_statistics).min(total)
            }
        };
        self.reviewed_lines += total - remaining;
        self.total_lines += total;
    }

    pub fn is_empty(self) -> bool {
        self.total_lines == 0
    }

    pub fn percent(self) -> u64 {
        self.share_of(100)
    }

    pub fn filled(self, cells: usize) -> usize {
        let filled = self.share_of(u64::try_from(cells).unwrap_or(u64::MAX));
        usize::try_from(filled).unwrap_or(cells).min(cells)
    }

    fn share_of(self, scale: u64) -> u64 {
        self.reviewed_lines
            .saturating_mul(scale)
            .checked_div(self.total_lines)
            .unwrap_or_default()
    }
}

impl<'a> FromIterator<(DiffStatistics, &'a ReviewState)> for ReviewProgress {
    fn from_iter<I: IntoIterator<Item = (DiffStatistics, &'a ReviewState)>>(files: I) -> Self {
        let mut progress = Self::default();
        for (statistics, state) in files {
            progress.add(statistics, state);
        }
        progress
    }
}

fn changed_lines(statistics: DiffStatistics) -> u64 {
    (statistics.lines_added + statistics.lines_removed).max(1)
}

#[cfg(test)]
#[path = "progress.tests.rs"]
mod tests;
