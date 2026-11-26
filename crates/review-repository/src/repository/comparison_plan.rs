use std::collections::{BTreeMap, BTreeSet};

use super::{DiffStatistics, RepoPath, SnapshotId};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BaselineComparisonPlan {
    baselines: BTreeMap<SnapshotId, BTreeSet<RepoPath>>,
}

impl BaselineComparisonPlan {
    pub fn add(&mut self, baseline_snapshot_id: SnapshotId, path: RepoPath) {
        self.baselines
            .entry(baseline_snapshot_id)
            .or_default()
            .insert(path);
    }

    pub(super) fn baselines(&self) -> impl Iterator<Item = (&SnapshotId, &BTreeSet<RepoPath>)> {
        self.baselines.iter()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BaselineComparison {
    Missing,
    Compared {
        path_statistics: BTreeMap<RepoPath, DiffStatistics>,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BaselineComparisonResults {
    baselines: BTreeMap<SnapshotId, BaselineComparison>,
}

impl BaselineComparisonResults {
    pub(super) fn insert(
        &mut self,
        baseline_snapshot_id: SnapshotId,
        comparison: BaselineComparison,
    ) {
        self.baselines.insert(baseline_snapshot_id, comparison);
    }

    pub fn get(&self, baseline_snapshot_id: &SnapshotId) -> Option<&BaselineComparison> {
        self.baselines.get(baseline_snapshot_id)
    }
}
