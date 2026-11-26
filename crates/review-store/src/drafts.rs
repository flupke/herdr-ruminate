use std::path::PathBuf;
use std::sync::Arc;

use review_threads::{ReviewThreads, SavedDrafts, ThreadSource};
use review_types::ReviewUnit;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::source_references::{SourceForm, SourceSlot, WithSources};
use super::{Error, Result, ReviewStore};

#[cfg(test)]
#[path = "drafts.tests.rs"]
mod tests;

const VERSION: u8 = 1;

#[derive(Deserialize, Serialize)]
struct StoredDrafts<Unit, Drafts> {
    version: u8,
    review_unit: Unit,
    drafts: Drafts,
}

impl WithSources for SavedDrafts {
    const SLOT: SourceSlot = SourceSlot::Field("source");

    fn sources_mut(&mut self) -> impl Iterator<Item = &mut Arc<ThreadSource>> {
        SavedDrafts::sources_mut(self)
    }

    fn records(json: &mut Value) -> Vec<&mut Value> {
        json.as_array_mut().into_iter().flatten().collect()
    }
}

impl ReviewStore {
    pub fn load_drafts(&self, review_unit: &ReviewUnit) -> Result<SavedDrafts> {
        let threads = self.load_threads(review_unit)?;
        let mut drafts = self.read_drafts(review_unit)?;
        drafts.forget_posted(&threads);
        Ok(drafts)
    }

    pub fn update_drafts<T>(
        &self,
        review_unit: &ReviewUnit,
        update: impl FnOnce(&mut SavedDrafts, &ReviewThreads) -> std::result::Result<T, String>,
    ) -> Result<(T, SavedDrafts)> {
        let threads = self.load_threads(review_unit)?;
        let _lock = self.drafts_lock(review_unit)?;
        let mut drafts = self.read_drafts(review_unit)?;
        let original = drafts.clone();
        drafts.forget_posted(&threads);
        let result = update(&mut drafts, &threads).map_err(Error::ThreadUpdate)?;
        if drafts != original {
            self.save_drafts(review_unit, &drafts)?;
        }
        Ok((result, drafts))
    }

    pub(super) fn adopt_embedded_drafts(
        &self,
        review_unit: &ReviewUnit,
        embedded: SavedDrafts,
    ) -> Result<()> {
        let _lock = self.drafts_lock(review_unit)?;
        let mut drafts = self.read_drafts(review_unit)?;
        let original = drafts.clone();
        for draft in embedded {
            drafts.keep(draft);
        }
        if drafts != original {
            self.save_drafts(review_unit, &drafts)?;
        }
        Ok(())
    }

    pub(super) fn read_drafts(&self, review_unit: &ReviewUnit) -> Result<SavedDrafts> {
        let path = self.drafts_path(review_unit)?;
        let Some(bytes) = Self::read_bytes(&path, "read drafts", None)? else {
            return Ok(SavedDrafts::default());
        };
        let json = Self::decode_thread_json(&path, &bytes)?;
        let stored: StoredDrafts<ReviewUnit, Value> =
            serde_json::from_slice(&json).map_err(Error::json("decode drafts", &path))?;
        if stored.version != VERSION || &stored.review_unit != review_unit {
            return Err(Error::InvalidStateKey {
                field: "draft storage version or review unit",
            });
        }
        self.attach(
            stored.drafts,
            SourceForm::Referenced,
            &path,
            "decode drafts",
        )
    }

    fn save_drafts(&self, review_unit: &ReviewUnit, drafts: &SavedDrafts) -> Result<()> {
        let detached = self.detach(drafts)?;
        self.atomic_referencing_json(
            &self.drafts_path(review_unit)?,
            &detached,
            &StoredDrafts {
                version: VERSION,
                review_unit,
                drafts: &detached.value,
            },
            "write drafts",
        )
    }

    fn drafts_lock(&self, review_unit: &ReviewUnit) -> Result<std::fs::File> {
        self.exclusive_lock(
            &self.drafts_path(review_unit)?.with_extension("lock"),
            "lock drafts",
        )
    }

    fn drafts_path(&self, review_unit: &ReviewUnit) -> Result<PathBuf> {
        self.review_record_path("drafts", review_unit)
    }
}
