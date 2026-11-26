use std::thread::JoinHandle;

use change_references_finder::ReferencesFinder;
use review_explore::{
    DispatchResult, ExploreHistory, ExploreRound, InterviewUpdate, ReviewCompletion, ViewSave,
};
use review_explore_round_settings::{RunAhead, WritingStyle};
use review_repository::repository::SnapshotIdentity;
use review_run_ahead::RoundForks;
use review_store::{Error, Result, ReviewStore};
use review_threads::{AskedUnder, Author, MessageId};
use review_types::ReviewUnit;

fn explore_error(reason: &str) -> Error {
    Error::Explore(reason.into())
}

fn historical_round_error() -> Error {
    explore_error("this round is history; open the latest round to continue")
}

pub(crate) struct EarlierRounds {
    pub(crate) reset_rounds: usize,
    pub(crate) standing: Vec<ExploreRound>,
}

pub(crate) struct Submitted {
    pub(crate) applied: bool,
    pub(crate) round: ExploreRound,
}

#[derive(Clone)]
pub(crate) struct SavedRounds {
    store: ReviewStore,
    references: Option<ReferencesFinder>,
}

impl SavedRounds {
    pub(crate) fn new(store: ReviewStore) -> Self {
        Self {
            store,
            references: None,
        }
    }

    pub(crate) fn with_references(self, references: Option<ReferencesFinder>) -> Self {
        Self { references, ..self }
    }

    // jj and gh may be slow, missing or offline.
    pub(crate) fn find_references(&self, round: &ExploreRound) -> Option<JoinHandle<()>> {
        let finder = self.references.clone()?;
        let comparison = &round.exploration.comparison;
        let base = comparison
            .base
            .clone()
            .filter(|base| matches!(base, SnapshotIdentity::Jj { .. }))?;
        let root = comparison.repository_root.clone();
        let unit = comparison.checkpoint.review_unit.clone();
        let instance = round.exploration.instance.clone();
        let store = self.store.clone();
        std::thread::Builder::new()
            .name("change-references".into())
            .spawn(move || {
                let references = finder.find(&root, &base);
                if let Ok(records) = store.lock_explore(&unit) {
                    let _ = records.save_change_references(&instance, &references);
                }
            })
            .ok()
    }

    pub(crate) fn next_writing(&self) -> Result<WritingStyle> {
        Ok(self.store.explore_round_settings()?.writing)
    }

    pub(crate) fn reviewer_notes(&self) -> Result<Option<String>> {
        self.store.explore_reviewer_notes()
    }

    pub(crate) fn run_ahead(&self) -> Result<RunAhead> {
        Ok(self.store.explore_round_settings()?.run_ahead)
    }

    pub(crate) fn forks(&self, unit: &ReviewUnit, instance: &str) -> Result<RoundForks> {
        self.store.load_round_forks(unit, instance)
    }

    pub(crate) fn rounds_with_forks(
        &self,
        wanted: impl Fn(&RoundForks) -> bool,
    ) -> Result<Vec<(ReviewUnit, String)>> {
        self.store.rounds_with_forks(wanted)
    }

    pub(crate) fn update_forks<T>(
        &self,
        unit: &ReviewUnit,
        instance: &str,
        update: impl FnOnce(&mut RoundForks) -> T,
    ) -> Result<T> {
        let records = self.store.lock_explore(unit)?;
        Ok(records.update_round_forks(instance, update)?.0)
    }

    pub(crate) fn reviewer_wrote_unknown_to_forks(
        &self,
        unit: &ReviewUnit,
        instance: &str,
        question: &str,
        since_ms: u64,
        taken_ms: u64,
    ) -> Result<bool> {
        let threads = self.store.load_threads(unit)?;
        let Some(conversation) = threads.round_conversation(instance) else {
            return Ok(false);
        };
        Ok(conversation.messages.iter().any(|message| {
            let under = matches!(
                &message.asked_under,
                Some(AskedUnder::Question { question: asked, .. }) if asked == question
            );
            let since = message
                .posted_at_ms
                .is_some_and(|posted| posted >= since_ms);
            let unknown_to_forks = message.posted_at_ms.is_none_or(|posted| posted >= taken_ms)
                || !conversation.is_answered(message);
            message.author == Author::Reviewer && (under || since) && unknown_to_forks
        }))
    }

    pub(crate) fn unanswered_round_messages(
        &self,
        unit: &ReviewUnit,
        instance: &str,
    ) -> Result<Vec<(MessageId, Option<u64>)>> {
        let threads = self.store.load_threads(unit)?;
        let Some(conversation) = threads.round_conversation(instance) else {
            return Ok(Vec::new());
        };
        Ok(conversation
            .messages
            .iter()
            .filter(|message| {
                message.author == Author::Reviewer && !conversation.is_answered(message)
            })
            .map(|message| (message.id.clone(), message.posted_at_ms))
            .collect())
    }

    pub(crate) fn history(&self, unit: &ReviewUnit) -> Result<ExploreHistory> {
        self.store.load_explore_history(unit)
    }

    pub(crate) fn round(&self, unit: &ReviewUnit, instance: &str) -> Result<Option<ExploreRound>> {
        self.store.load_explore(unit, instance)
    }

    pub(crate) fn earlier(&self, unit: &ReviewUnit, instance: &str) -> Result<EarlierRounds> {
        let history = self.history(unit)?;
        let before = history.before(instance);
        Ok(EarlierRounds {
            reset_rounds: before.reset,
            standing: before
                .standing
                .iter()
                .filter_map(|saved| self.round(unit, saved).ok().flatten())
                .collect(),
        })
    }

    pub(crate) fn view(&self, unit: &ReviewUnit, instance: &str) -> Result<Option<ViewSave>> {
        self.store.load_explore_view(unit, instance)
    }

    pub(crate) fn create(&self, round: ExploreRound) -> Result<ExploreRound> {
        let records = self
            .store
            .lock_explore(&round.exploration.comparison.checkpoint.review_unit)?;
        let mut history = records.history()?;
        records.create_round(&round)?;
        if history.closed {
            history.reset_rounds = history.rounds.len();
        }
        history.rounds.push(round.exploration.instance.clone());
        history.latest_editable = true;
        history.closed = false;
        records.save_history(&history)?;
        Ok(round)
    }

    pub(crate) fn update<T>(
        &self,
        unit: &ReviewUnit,
        instance: &str,
        update: impl FnOnce(&mut ExploreRound) -> std::result::Result<T, String>,
    ) -> Result<(T, ExploreRound)> {
        let records = self.store.lock_explore(unit)?;
        if records.history()?.is_historical(instance) {
            return Err(historical_round_error());
        }
        records.update_round(instance, update)
    }

    pub(crate) fn submit(
        &self,
        unit: &ReviewUnit,
        instance: &str,
        update: &InterviewUpdate,
        diverged: Vec<String>,
    ) -> Result<Submitted> {
        let records = self.store.lock_explore(unit)?;
        if records.history()?.is_historical(instance) {
            return Err(historical_round_error());
        }
        let (applied, round) = records.update_round(instance, |round| {
            let applied = round.submit(update).map_err(|error| error.to_string())?;
            if applied && let Some(turn) = round.turns.get_mut(&update.request) {
                merge_diverged(&mut turn.diverged, diverged);
            }
            if applied && update.conclusion.is_some() && round.completion.is_none() {
                round.completion = Some(ReviewCompletion {
                    request: update.request.clone(),
                    baseline: round.exploration.comparison.checkpoint.checkpoint.clone(),
                });
            }
            Ok(applied)
        })?;
        Ok(Submitted { applied, round })
    }

    pub(crate) fn revise(
        &self,
        unit: &ReviewUnit,
        instance: &str,
        revision: &review_explore::QuestionRevision,
    ) -> Result<Submitted> {
        let records = self.store.lock_explore(unit)?;
        if records.history()?.is_historical(instance) {
            return Err(historical_round_error());
        }
        let (applied, round) = records.update_round(instance, |round| {
            round
                .exploration
                .revise(revision.clone())
                .map_err(|error| error.to_string())
        })?;
        Ok(Submitted { applied, round })
    }

    pub(crate) fn finish_dispatch(
        &self,
        unit: &ReviewUnit,
        instance: &str,
        result: &DispatchResult,
    ) -> Result<ExploreRound> {
        let records = self.store.lock_explore(unit)?;
        if !records.history()?.rounds.iter().any(|id| id == instance) {
            return Err(explore_error("saved round is missing"));
        }
        records
            .update_round(instance, |round| {
                round.finish_dispatch(result);
                Ok(())
            })
            .map(|((), round)| round)
    }

    pub(crate) fn repair_history(&self, unit: &ReviewUnit) -> Result<ExploreHistory> {
        let records = self.store.lock_explore(unit)?;
        let mut rounds: Vec<_> = records
            .round_files()?
            .into_iter()
            .filter(|(instance, _)| records.round(instance).ok().flatten().is_some())
            .map(|(instance, modified)| (modified, instance))
            .collect();
        rounds.sort();
        let history = ExploreHistory {
            rounds: rounds.into_iter().map(|(_, instance)| instance).collect(),
            latest_editable: false,
            ..ExploreHistory::default()
        };
        records.save_history(&history)?;
        Ok(history)
    }

    pub(crate) fn close(&self, unit: &ReviewUnit, instance: &str) -> Result<()> {
        let records = self.store.lock_explore(unit)?;
        let mut history = records.history()?;
        if history.latest() != Some(instance) || history.closed {
            return Ok(());
        }
        history.closed = true;
        records.save_history(&history)?;
        records.sync()
    }

    pub(crate) fn clear_round(&self, unit: &ReviewUnit, instance: &str) -> Result<()> {
        let records = self.store.lock_explore(unit)?;
        let mut history = records.history()?;
        if history.rounds.last().is_some_and(|saved| saved == instance) {
            history.latest_editable = false;
        }
        history.rounds.retain(|saved| saved != instance);
        records.save_history(&history)?;
        records.remove_round(instance)?;
        records.remove_view(instance)?;
        records.remove_round_forks(instance)?;
        records.sync()
    }

    pub(crate) fn clear_view(&self, unit: &ReviewUnit, instance: &str) -> Result<()> {
        let records = self.store.lock_explore(unit)?;
        records.remove_view(instance)?;
        records.sync()
    }

    pub(crate) fn save_view(&self, unit: &ReviewUnit, view: &ViewSave) -> Result<()> {
        if &view.review_unit != unit {
            return Err(explore_error("editor belongs to another review"));
        }
        let records = self.store.lock_explore(unit)?;
        if !records.history()?.rounds.contains(&view.instance) {
            return Err(explore_error("saved round is missing"));
        }
        if let Some(previous) = records.view(&view.instance)? {
            if previous.sequence == view.sequence && previous != *view {
                return Err(explore_error(
                    "editor sequence already has different content; original retained",
                ));
            }
            if previous.sequence >= view.sequence {
                return Ok(());
            }
        }
        records.save_view(view)
    }
}

#[cfg(test)]
#[path = "records.tests.rs"]
mod tests;

pub(crate) fn merge_diverged(diverged: &mut Vec<String>, more: Vec<String>) {
    for path in more {
        if !diverged.contains(&path) {
            diverged.push(path);
        }
    }
}
