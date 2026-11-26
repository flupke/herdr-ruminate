use std::path::Path;
use std::sync::{Arc, LazyLock};

use review_source::{AnchorKind, DiffRangeAnchor};
use review_threads::ThreadSource;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::thread_sources::StoredSource;
use super::{Error, Result, ReviewStore};

#[derive(Clone, Copy)]
pub(super) enum SourceSlot {
    Merged,
    Field(&'static str),
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum SourceForm {
    Inline,
    Referenced,
}

pub(super) trait WithSources: Clone + Serialize + DeserializeOwned {
    const SLOT: SourceSlot;

    fn sources_mut(&mut self) -> impl Iterator<Item = &mut Arc<ThreadSource>>;

    fn records(json: &mut Value) -> Vec<&mut Value>;
}

struct StandIn {
    source: Arc<ThreadSource>,
    fields: Map<String, Value>,
    json: Vec<u8>,
}

static STAND_IN: LazyLock<StandIn> = LazyLock::new(|| {
    let source = Arc::new(ThreadSource {
        anchor: DiffRangeAnchor {
            source_checkpoint: "stand-in for a stored source reference".into(),
            old_path: None,
            new_path: None,
            old_lines: None,
            new_lines: None,
            target_kind: AnchorKind::default(),
            source_hunk_count: 0,
            old_content: None,
            new_content: None,
            diff_hash: String::new(),
        },
        excerpt: String::new(),
    });
    let Ok(Value::Object(fields)) = serde_json::to_value(&*source) else {
        unreachable!("a thread source serializes as a JSON object");
    };
    let json = serde_json::to_vec(&*source).expect("a thread source serializes");
    StandIn {
        source,
        fields,
        json,
    }
});

impl SourceSlot {
    fn bytes(self, object: &[u8]) -> &[u8] {
        match self {
            Self::Merged => &object[1..object.len() - 1],
            Self::Field(_) => object,
        }
    }

    fn holds_reference(self, record: &Value) -> bool {
        match self {
            Self::Merged => StoredSource::deserialize(record).is_ok(),
            Self::Field(_) => true,
        }
    }

    fn take_reference(self, record: &mut Value) -> serde_json::Result<StoredSource> {
        let invalid = || serde::de::Error::custom("a stored record is not an object");
        match self {
            Self::Merged => {
                let reference = StoredSource::deserialize(&*record)?;
                let object = record.as_object_mut().ok_or_else(invalid)?;
                if let Value::Object(fields) = serde_json::to_value(&reference)? {
                    for key in fields.keys() {
                        object.remove(key);
                    }
                }
                object.extend(STAND_IN.fields.clone());
                Ok(reference)
            }
            Self::Field(name) => {
                let field = record
                    .as_object_mut()
                    .ok_or_else(invalid)?
                    .get_mut(name)
                    .ok_or_else(|| serde::de::Error::missing_field(name))?;
                let reference = StoredSource::deserialize(&*field)?;
                *field = Value::Object(STAND_IN.fields.clone());
                Ok(reference)
            }
        }
    }
}

pub(super) struct Detached<T> {
    pub(super) value: T,
    references: Vec<StoredSource>,
}

impl<T: WithSources> Detached<T> {
    fn json(&self, document: &impl Serialize) -> serde_json::Result<Vec<u8>> {
        let json = serde_json::to_vec(document)?;
        let stand_in = T::SLOT.bytes(&STAND_IN.json);
        let mut references = self.references.iter();
        let mut written = Vec::with_capacity(json.len());
        let mut rest = json.as_slice();
        // The stand-in cannot occur inside a JSON string, where every quote is escaped.
        while let Some(at) = rest
            .windows(stand_in.len())
            .position(|window| window == stand_in)
        {
            let reference = references.next().ok_or_else(Self::mismatch)?;
            written.extend_from_slice(&rest[..at]);
            written.extend_from_slice(T::SLOT.bytes(&serde_json::to_vec(reference)?));
            rest = &rest[at + stand_in.len()..];
        }
        if references.next().is_some() {
            return Err(Self::mismatch());
        }
        written.extend_from_slice(rest);
        Ok(written)
    }

    fn mismatch() -> serde_json::Error {
        serde::ser::Error::custom("the document does not hold each detached source once")
    }
}

impl ReviewStore {
    pub(super) fn detach<T: WithSources>(&self, value: &T) -> Result<Detached<T>> {
        let mut value = value.clone();
        let references = value
            .sources_mut()
            .map(|source| {
                let reference = self.save_thread_source(source)?;
                *source = STAND_IN.source.clone();
                Ok(reference)
            })
            .collect::<Result<_>>()?;
        Ok(Detached { value, references })
    }

    pub(super) fn atomic_referencing_json<T: WithSources>(
        &self,
        target: &Path,
        detached: &Detached<T>,
        document: &impl Serialize,
        operation: &'static str,
    ) -> Result<()> {
        let json = detached
            .json(document)
            .map_err(Error::json(operation, target))?;
        self.atomic_compressed_bytes(target, &json, operation)
    }

    pub(super) fn attach<T: WithSources>(
        &self,
        mut json: Value,
        form: SourceForm,
        path: &Path,
        operation: &'static str,
    ) -> Result<T> {
        let decode = Error::json(operation, path);
        let references = match form {
            SourceForm::Referenced => T::records(&mut json)
                .into_iter()
                .filter(|record| T::SLOT.holds_reference(record))
                .map(|record| T::SLOT.take_reference(record))
                .collect::<serde_json::Result<Vec<_>>>()
                .map_err(&decode)?,
            SourceForm::Inline => Vec::new(),
        };
        let mut value: T = serde_json::from_value(json).map_err(&decode)?;
        if form == SourceForm::Referenced {
            let mut references = references.into_iter();
            for source in value.sources_mut() {
                let reference = references.next().ok_or_else(|| {
                    decode(serde::de::Error::custom("a stored source has no reference"))
                })?;
                *source = self.load_thread_source(reference)?;
            }
        }
        Ok(value)
    }
}
