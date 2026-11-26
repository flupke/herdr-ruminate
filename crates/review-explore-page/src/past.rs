use std::sync::Arc;

use change_references::ChangeReferences;
use review_repository::repository::ShortRevision;
use review_threads::ReviewThreads;
use serde::Serialize;
use tokio::sync::watch;
use ts_rs::TS;

use crate::conversation::ThreadsSnapshot;
use crate::round::{PublishedRound, ReviewName, RoundSnapshot, RoundStage};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, TS)]
pub struct PastChange {
    pub title: String,
    pub references: ChangeReferences,
    pub revision: ShortRevision,
    pub rounds: Vec<PastRoundEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, TS)]
pub struct PastRoundEntry {
    pub id: String,
    pub saved_at_ms: u64,
}

#[derive(Clone, Debug, Default)]
struct PastList {
    revision: u64,
    changes: Arc<[PastChange]>,
}

pub struct PastRoundsPublisher(watch::Sender<PastList>);

impl PastRoundsPublisher {
    pub fn publish(&self, changes: Vec<PastChange>) {
        self.0.send_if_modified(|list| {
            if *list.changes == *changes {
                return false;
            }
            list.revision += 1;
            list.changes = changes.into();
            true
        });
    }
}

impl Default for PastRoundsPublisher {
    fn default() -> Self {
        Self(watch::Sender::new(PastList::default()))
    }
}

type Open = Arc<dyn Fn(&str) -> Option<PastRound> + Send + Sync>;

#[derive(Clone)]
pub struct PastRounds {
    list: watch::Receiver<PastList>,
    open: Open,
}

impl PastRounds {
    pub fn new(
        publisher: &PastRoundsPublisher,
        open: impl Fn(&str) -> Option<PastRound> + Send + Sync + 'static,
    ) -> Self {
        Self {
            list: publisher.0.subscribe(),
            open: Arc::new(open),
        }
    }

    pub(crate) fn revision(&self) -> u64 {
        self.list.borrow().revision
    }

    pub(crate) fn listed(&self, followed: Option<&str>) -> Vec<PastChange> {
        let changes = self.list.borrow().changes.clone();
        changes
            .iter()
            .filter_map(|change| {
                let rounds: Vec<_> = change
                    .rounds
                    .iter()
                    .filter(|round| Some(round.id.as_str()) != followed)
                    .cloned()
                    .collect();
                (!rounds.is_empty()).then(|| PastChange {
                    rounds,
                    ..change.clone()
                })
            })
            .collect()
    }

    pub(crate) fn saved_at(&self, id: &str) -> Option<u64> {
        self.list
            .borrow()
            .changes
            .iter()
            .flat_map(|change| &change.rounds)
            .find(|round| round.id == id)
            .map(|round| round.saved_at_ms)
    }

    pub(crate) async fn open(&self, id: String) -> Option<PastRound> {
        let open = self.open.clone();
        tokio::task::spawn_blocking(move || open(&id))
            .await
            .ok()
            .flatten()
    }

    pub(crate) async fn changed(&mut self) -> bool {
        self.list.changed().await.is_ok()
    }
}

#[derive(Clone, Debug)]
pub struct PastRound {
    pub(crate) snapshot: RoundSnapshot,
    review: ReviewName,
    pub(crate) references: ChangeReferences,
    pub(crate) threads: ThreadsSnapshot,
    pub(crate) saved_at_ms: u64,
}

impl PastRound {
    pub fn new(
        round: PublishedRound<'_>,
        stage: RoundStage,
        review: ReviewName,
        references: ChangeReferences,
        threads: Option<ReviewThreads>,
        saved_at_ms: u64,
    ) -> Self {
        let round = PublishedRound {
            cancellable: None,
            earlier: true,
            ..round
        };
        Self {
            snapshot: RoundSnapshot::past(round, stage, review.clone()),
            review,
            references,
            threads: ThreadsSnapshot::of(threads),
            saved_at_ms,
        }
    }

    pub fn id(&self) -> &str {
        self.snapshot.round.as_deref().unwrap_or_default()
    }

    pub fn stage(&self) -> &RoundStage {
        &self.snapshot.stage
    }

    pub fn review(&self) -> &ReviewName {
        &self.review
    }

    pub fn saved_at_ms(&self) -> u64 {
        self.saved_at_ms
    }

    pub fn references(&self) -> &ChangeReferences {
        &self.references
    }
}
