use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use grep_matcher::Matcher;
use grep_regex::{RegexMatcher, RegexMatcherBuilder};

mod ripgrep;
mod worker;

pub use worker::Worker;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Position {
    pub row: usize,
    pub column: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Match {
    pub document_index: usize,
    pub position: Position,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Document {
    text: String,
    rows: Vec<Range<usize>>,
}

impl Document {
    pub fn from_rows(rows: impl IntoIterator<Item = String>) -> Self {
        let mut document = Self::default();
        for row in rows {
            let start = document.text.len();
            document.text.push_str(&row);
            document.rows.push(start..document.text.len());
            document.text.push('\n');
        }
        document
    }

    fn position(&self, offset: usize) -> Option<Position> {
        let row = self
            .rows
            .partition_point(|range| range.start <= offset)
            .checked_sub(1)?;
        let range = &self.rows[row];
        (offset < range.end).then_some(Position {
            row,
            column: offset - range.start,
        })
    }
}

pub struct Query {
    pattern: Option<RegexMatcher>,
}

impl Query {
    pub fn new(text: &str) -> Self {
        let pattern = (!text.is_empty())
            .then(|| {
                RegexMatcherBuilder::new()
                    .fixed_strings(true)
                    .case_smart(true)
                    .line_terminator((!text.contains('\n')).then_some(b'\n'))
                    .build(text)
                    .ok()
            })
            .flatten();
        Self { pattern }
    }

    pub fn ranges<'a>(&'a self, text: &'a str) -> impl Iterator<Item = Range<usize>> + 'a {
        self.byte_ranges(text.as_bytes())
    }

    fn byte_ranges<'a>(&'a self, text: &'a [u8]) -> impl Iterator<Item = Range<usize>> + 'a {
        let mut offset = 0;
        std::iter::from_fn(move || {
            let found = self.pattern.as_ref()?.find_at(text, offset).ok()??;
            offset = found.end();
            Some(found.start()..found.end())
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Request {
    id: u64,
    pub query: String,
    pub documents: Vec<Arc<Document>>,
}

impl Request {
    pub fn new(query: String, documents: Vec<Arc<Document>>) -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            query,
            documents,
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn same_documents(&self, documents: &[Arc<Document>]) -> bool {
        self.documents.len() == documents.len()
            && self
                .documents
                .iter()
                .zip(documents)
                .all(|(before, after)| Arc::ptr_eq(before, after))
    }

    pub fn is_large(&self) -> bool {
        self.documents
            .iter()
            .map(|document| document.text.len())
            .sum::<usize>()
            > 64 * 1024
    }

    /// # Panics
    /// Panics if an uncancellable search unexpectedly reports cancellation.
    pub fn search(&self) -> Results {
        self.search_unless(|| false)
            .expect("search is not cancelled")
    }

    fn search_unless(&self, cancelled: impl Fn() -> bool) -> Option<Results> {
        let query = Query::new(&self.query);
        let mut matches = Vec::new();
        for (document_index, document) in self.documents.iter().enumerate() {
            for (row, range) in document.rows.iter().enumerate() {
                if cancelled() {
                    return None;
                }
                for found in query.ranges(&document.text[range.clone()]) {
                    if cancelled() {
                        return None;
                    }
                    matches.push(Match {
                        document_index,
                        position: Position {
                            row,
                            column: found.start,
                        },
                    });
                }
            }
        }
        Some(Results {
            id: self.id,
            matches,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Results {
    pub id: u64,
    pub matches: Vec<Match>,
}
