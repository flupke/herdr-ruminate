use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::time::{SystemTime, UNIX_EPOCH};

use change_references::ChangeReferences;
use review_explore::{EvidenceRef, ExploreRound, RoundHeading, RoundOverview};
use review_explore_page::{
    PastChange, PastRound, PastRoundEntry, PastRounds, PastRoundsPublisher, PublishedRound,
    ReviewName, RoundStage,
};
use review_repository::repository::{Repository, SnapshotIdentity};
use review_store::{ReviewStore, SavedRoundFile};

use crate::page::{CitedCode, LatestTurn, PageCitations, interrupted, latest_turn, sent_answer};

const NO_DESCRIPTION: &str = "No description";

pub struct SavedRoundsReader {
    store: ReviewStore,
    repository: Repository,
    headings: Mutex<HashMap<PathBuf, (SystemTime, Option<RoundHeading>)>>,
}

impl SavedRoundsReader {
    pub fn new(store: ReviewStore, repository: Repository) -> Self {
        Self {
            store,
            repository,
            headings: Mutex::default(),
        }
    }

    pub fn serve(self) -> (PastRounds, PastRoundsRefresh) {
        let reader = Arc::new(self);
        let publisher = PastRoundsPublisher::default();
        let opener = reader.clone();
        let rounds = PastRounds::new(&publisher, move |id| opener.open(id));
        (rounds, PastRoundsRefresh::start(reader, publisher))
    }

    pub(crate) fn list(&self) -> Vec<PastChange> {
        let mut by_review: BTreeMap<String, Vec<(SystemTime, RoundHeading)>> = BTreeMap::new();
        for (saved_at, heading) in self.headings() {
            by_review
                .entry(heading.review_unit.as_str().to_owned())
                .or_default()
                .push((saved_at, heading));
        }
        let mut changes: Vec<(SystemTime, PastChange)> = by_review
            .into_values()
            .filter_map(|mut rounds| {
                newest_first(&mut rounds);
                let (newest, heading) = rounds.first()?;
                let references = self.change_references(rounds.iter().map(|(_, heading)| heading));
                let change = PastChange {
                    title: title(heading, &references),
                    references,
                    revision: heading
                        .base
                        .as_ref()
                        .map(SnapshotIdentity::short_revision)
                        .unwrap_or_default(),
                    rounds: rounds
                        .iter()
                        .map(|(saved_at, heading)| PastRoundEntry {
                            id: heading.instance.clone(),
                            saved_at_ms: millis(*saved_at),
                        })
                        .collect(),
                };
                Some((*newest, change))
            })
            .collect();
        changes.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.title.cmp(&b.1.title)));
        changes.into_iter().map(|(_, change)| change).collect()
    }

    pub(crate) fn open(&self, id: &str) -> Option<PastRound> {
        let mut rounds = self.headings();
        let (saved_at, heading) = rounds
            .iter()
            .find(|(_, heading)| heading.instance == id)
            .cloned()?;
        let round = self
            .store
            .load_explore(&heading.review_unit, &heading.instance)
            .ok()
            .flatten()?;
        let threads = self.store.load_threads(&heading.review_unit).ok();
        rounds.retain(|(_, round)| round.review_unit == heading.review_unit);
        newest_first(&mut rounds);
        let references = self.change_references(rounds.iter().map(|(_, round)| round));
        let review = ReviewName {
            title: title(&heading, &references),
            ..heading.base.as_ref().map_or_else(
                || ReviewName::unnamed(self.repository.root()),
                |base| ReviewName::of(self.repository.root(), base),
            )
        };
        Some(past_round(
            &round,
            &self.repository,
            review,
            references,
            threads,
            millis(saved_at),
        ))
    }

    fn change_references<'a>(
        &self,
        rounds: impl IntoIterator<Item = &'a RoundHeading>,
    ) -> ChangeReferences {
        let saved: Vec<ChangeReferences> = rounds
            .into_iter()
            .filter_map(|heading| {
                self.store
                    .load_change_references(&heading.review_unit, &heading.instance)
                    .ok()
                    .flatten()
            })
            .collect();
        ChangeReferences::of_rounds(&saved)
    }

    fn headings(&self) -> Vec<(SystemTime, RoundHeading)> {
        let files = self.store.saved_explore_round_files().unwrap_or_default();
        let mut headings = self.headings.lock().unwrap_or_else(PoisonError::into_inner);
        headings.retain(|path, _| files.iter().any(|file| file.path() == path));
        files
            .iter()
            .filter_map(|file: &SavedRoundFile| {
                let read = headings
                    .get(file.path())
                    .filter(|(saved_at, _)| *saved_at == file.saved_at())
                    .map(|(_, heading)| heading.clone());
                let heading = read.unwrap_or_else(|| {
                    let heading = self.store.read_round_heading(file);
                    headings.insert(file.path().to_owned(), (file.saved_at(), heading.clone()));
                    heading
                });
                Some((file.saved_at(), heading?))
            })
            .collect()
    }
}

#[derive(Clone)]
pub struct PastRoundsRefresh(mpsc::Sender<()>);

impl PastRoundsRefresh {
    fn start(reader: Arc<SavedRoundsReader>, publisher: PastRoundsPublisher) -> Self {
        let (refresh, refreshes) = mpsc::channel::<()>();
        let _ = std::thread::Builder::new()
            .name("past-rounds".into())
            .spawn(move || {
                publisher.publish(reader.list());
                while refreshes.recv().is_ok() {
                    while refreshes.try_recv().is_ok() {}
                    publisher.publish(reader.list());
                }
            });
        Self(refresh)
    }

    pub fn refresh(&self) {
        let _ = self.0.send(());
    }
}

fn past_round(
    round: &ExploreRound,
    repository: &Repository,
    review: ReviewName,
    references: ChangeReferences,
    threads: Option<review_threads::ReviewThreads>,
    saved_at_ms: u64,
) -> PastRound {
    let exploration = &round.exploration;
    let comparison = &exploration.comparison;
    let code = CitedCode::Saved(repository);
    let mut citations = PageCitations::default();
    let stage = saved_stage(round, &mut citations, code);
    let overview = RoundOverview::of(round, None);
    let lists: Vec<Vec<EvidenceRef>> = overview
        .earlier
        .iter()
        .map(|record| record.question.evidence.clone())
        .collect();
    let earlier_citations = PageCitations::default().lists(&lists, comparison, code);
    let published = PublishedRound {
        id: &exploration.instance,
        review_unit: &comparison.checkpoint.review_unit,
        design: exploration.design(),
        changed_files: comparison.files.len(),
        author: exploration.author,
        cancellable: None,
        earlier: true,
        overview: &overview,
        earlier_citations: &earlier_citations,
    };
    PastRound::new(published, stage, review, references, threads, saved_at_ms)
}

fn saved_stage(
    round: &ExploreRound,
    citations: &mut PageCitations,
    code: CitedCode<'_>,
) -> RoundStage {
    let exploration = &round.exploration;
    if let Some(request) = exploration
        .pending_request()
        .or_else(|| exploration.retry_request())
    {
        let answer = sent_answer(round, request, citations, code);
        return interrupted(round, request, answer);
    }
    latest_turn(
        round,
        LatestTurn {
            sending: false,
            takes_quiz_answers: false,
            paths: &BTreeMap::new(),
        },
        citations,
        code,
    )
}

fn title(heading: &RoundHeading, references: &ChangeReferences) -> String {
    references
        .title()
        .or_else(|| {
            heading
                .base
                .as_ref()
                .map(|base| base.title().trim())
                .filter(|title| !title.is_empty())
        })
        .unwrap_or(NO_DESCRIPTION)
        .to_owned()
}

fn newest_first(rounds: &mut [(SystemTime, RoundHeading)]) {
    rounds.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.instance.cmp(&b.1.instance)));
}

fn millis(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH).map_or(0, |elapsed| {
        u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
    })
}
