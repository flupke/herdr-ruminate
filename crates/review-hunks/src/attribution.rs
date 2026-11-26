use std::collections::BTreeMap;
use std::ops::Range;

use review_types::MarkAuthor;

use crate::text::{Lines, changes};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthoredLines {
    pub lines: Range<u32>,
    pub author: MarkAuthor,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Attribution {
    pub default: MarkAuthor,
    pub removed: Vec<AuthoredLines>,
    pub added: Vec<AuthoredLines>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Reviewed {
    pub text: Vec<u8>,
    pub attribution: Attribution,
}

pub(crate) static REVIEWER_ONLY: Attribution = Attribution {
    default: MarkAuthor::Reviewer,
    removed: Vec::new(),
    added: Vec::new(),
};

impl Attribution {
    pub fn uniform(author: MarkAuthor) -> Self {
        Self {
            default: author,
            removed: Vec::new(),
            added: Vec::new(),
        }
    }

    pub fn uniform_author(&self) -> Option<&MarkAuthor> {
        (self.removed.is_empty() && self.added.is_empty()).then_some(&self.default)
    }

    pub fn removed_by(&self, line: u32) -> &MarkAuthor {
        Self::author(&self.removed, line).unwrap_or(&self.default)
    }

    pub fn added_by(&self, line: u32) -> &MarkAuthor {
        Self::author(&self.added, line).unwrap_or(&self.default)
    }

    fn author(entries: &[AuthoredLines], line: u32) -> Option<&MarkAuthor> {
        entries
            .iter()
            .find(|entry| entry.lines.contains(&line))
            .map(|entry| &entry.author)
    }

    fn runs(&self, authors: impl IntoIterator<Item = (u32, MarkAuthor)>) -> Vec<AuthoredLines> {
        let mut runs: Vec<AuthoredLines> = Vec::new();
        for (line, author) in authors {
            if author == self.default {
                continue;
            }
            match runs.last_mut() {
                Some(last) if last.lines.end == line && last.author == author => {
                    last.lines.end += 1;
                }
                _ => runs.push(AuthoredLines {
                    lines: line..line + 1,
                    author,
                }),
            }
        }
        runs
    }
}

pub(crate) struct Rebuild {
    default: MarkAuthor,
    text: Vec<u8>,
    authors: Vec<MarkAuthor>,
    removed: BTreeMap<u32, MarkAuthor>,
}

impl Rebuild {
    pub(crate) fn on(previous: &Attribution) -> Self {
        let mut rebuild = Self::with_default(previous.default.clone());
        for entry in &previous.removed {
            for line in entry.lines.clone() {
                rebuild.remove(line, &entry.author);
            }
        }
        rebuild
    }

    pub(crate) fn with_default(default: MarkAuthor) -> Self {
        Self {
            default,
            text: Vec::new(),
            authors: Vec::new(),
            removed: BTreeMap::new(),
        }
    }

    pub(crate) fn push(&mut self, line: &[u8], author: &MarkAuthor) {
        self.text.extend_from_slice(line);
        self.authors.push(author.clone());
    }

    pub(crate) fn keep(
        &mut self,
        lines: &Lines<'_>,
        range: Range<u32>,
        attribution: &Attribution,
    ) -> Option<()> {
        for (line, text) in range.clone().zip(lines.get(range)?) {
            self.push(text, attribution.added_by(line));
        }
        Some(())
    }

    pub(crate) fn remove(&mut self, line: u32, author: &MarkAuthor) {
        self.removed.insert(line, author.clone());
    }

    pub(crate) fn finish(self, base: &[u8]) -> Reviewed {
        let changes = changes(base, &self.text);
        let added = changes
            .iter()
            .flat_map(|change| {
                change.after.clone().map(|line| {
                    let author = usize::try_from(line)
                        .ok()
                        .and_then(|index| self.authors.get(index))
                        .unwrap_or(&self.default);
                    (line, author.clone())
                })
            })
            .collect::<Vec<_>>();
        let removed = changes
            .iter()
            .flat_map(|change| {
                change.before.clone().map(|line| {
                    let author = self.removed.get(&line).unwrap_or(&self.default);
                    (line, author.clone())
                })
            })
            .collect::<Vec<_>>();
        let mut authors = added.iter().chain(&removed).map(|(_, author)| author);
        let default = match authors.next() {
            Some(first) if authors.all(|author| author == first) => first.clone(),
            _ => self.default,
        };
        let mut attribution = Attribution::uniform(default);
        attribution.added = attribution.runs(added);
        attribution.removed = attribution.runs(removed);
        Reviewed {
            text: self.text,
            attribution,
        }
    }
}

#[cfg(test)]
#[path = "attribution.tests.rs"]
mod tests;
