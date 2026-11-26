use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;

#[derive(Clone, Debug)]
pub struct TurnLog {
    directory: PathBuf,
    recorded: Arc<AtomicU64>,
}

#[derive(Serialize)]
struct RecordedTurn<'a, T> {
    turn: u64,
    delivered: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(flatten)]
    sent: &'a T,
}

impl TurnLog {
    pub fn open(directory: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(&directory)?;
        let recorded = std::fs::read_dir(&directory)?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                entry
                    .file_name()
                    .to_str()?
                    .strip_prefix("turn-")?
                    .strip_suffix(".json")?
                    .parse::<u64>()
                    .ok()
            })
            .max()
            .unwrap_or(0);
        Ok(Self {
            directory,
            recorded: Arc::new(AtomicU64::new(recorded)),
        })
    }

    pub fn record(&self, sent: &impl Serialize, error: Option<String>) {
        let turn = self.recorded.fetch_add(1, Ordering::AcqRel) + 1;
        let recorded = RecordedTurn {
            turn,
            delivered: error.is_none(),
            error,
            sent,
        };
        let path = self.directory.join(format!("turn-{turn:06}.json"));
        let partial = path.with_extension("partial");
        if let Ok(bytes) = serde_json::to_vec_pretty(&recorded)
            && std::fs::write(&partial, bytes).is_ok()
        {
            let _ = std::fs::rename(partial, path);
        }
    }
}

#[cfg(test)]
#[path = "lib.tests.rs"]
mod tests;
