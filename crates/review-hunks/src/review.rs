use review_types::MarkAuthor;

use crate::attribution::{Attribution, REVIEWER_ONLY, Rebuild, Reviewed};
use crate::hunks::HunkSpan;
use crate::text::{Lines, before_line, changes, overlaps, translate};

pub struct HunkReview<'a> {
    pub(crate) base: &'a [u8],
    pub(crate) reviewed: &'a [u8],
    pub(crate) current: &'a [u8],
    pub(crate) attribution: &'a Attribution,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ChangedLines {
    pub removed: Vec<u32>,
    pub added: Vec<u32>,
    pub rewrites_reviewed: bool,
}

pub struct BaseNumbering(Vec<crate::text::Change>);

impl BaseNumbering {
    pub fn base_line(&self, reviewed_line: u32) -> Option<u32> {
        before_line(&self.0, reviewed_line)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReviewedVersion {
    Current(Attribution),
    Base,
    Partial(Reviewed),
}

impl<'a> HunkReview<'a> {
    pub fn new(base: &'a [u8], reviewed: &'a [u8], current: &'a [u8]) -> Self {
        Self {
            base,
            reviewed,
            current,
            attribution: &REVIEWER_ONLY,
        }
    }

    #[must_use]
    pub fn attributed(self, attribution: &'a Attribution) -> Self {
        Self {
            attribution,
            ..self
        }
    }

    pub fn review(&self, open: &HunkSpan, author: &MarkAuthor) -> Option<ReviewedVersion> {
        self.review_all(std::slice::from_ref(open), author)
    }

    pub fn review_all(&self, open: &[HunkSpan], author: &MarkAuthor) -> Option<ReviewedVersion> {
        let reviewed = Lines::new(self.reviewed);
        let current = Lines::new(self.current);
        let reviewed_changes = changes(self.base, self.reviewed);
        let mut spans = open.iter().collect::<Vec<_>>();
        spans.sort_by_key(|span| span.old.start);
        let mut rebuild = Rebuild::on(self.attribution);
        let mut position = 0;
        for span in spans {
            rebuild.keep(&reviewed, position..span.old.start, self.attribution)?;
            let replaced = reviewed.get(span.old.clone())?;
            let added = current.get(span.new.clone())?;
            let inside = changes(&replaced.concat(), &added.concat());
            for change in &inside {
                for line in change.before.clone() {
                    if let Some(base) = before_line(&reviewed_changes, span.old.start + line) {
                        rebuild.remove(base, author);
                    }
                }
            }
            for (offset, text) in (0..).zip(added) {
                let author = match before_line(&inside, offset) {
                    Some(kept) => self.attribution.added_by(span.old.start + kept),
                    None => author,
                };
                rebuild.push(text, author);
            }
            position = span.old.end;
        }
        rebuild.keep(&reviewed, position..reviewed.len(), self.attribution)?;
        Some(self.version(rebuild.finish(self.base)))
    }

    pub fn base_numbering(&self) -> BaseNumbering {
        BaseNumbering(changes(self.base, self.reviewed))
    }

    pub fn changed_lines(&self, open: &[HunkSpan]) -> Vec<ChangedLines> {
        let reviewed_changes = changes(self.base, self.reviewed);
        let open_changes = changes(self.reviewed, self.current);
        open.iter()
            .map(|span| {
                let mut lines = ChangedLines::default();
                for change in open_changes
                    .iter()
                    .filter(|change| overlaps(&change.before, &span.old))
                {
                    for line in change.before.clone() {
                        match before_line(&reviewed_changes, line) {
                            Some(base) => lines.removed.push(base),
                            None => lines.rewrites_reviewed = true,
                        }
                    }
                    lines.added.extend(change.after.clone());
                }
                lines
            })
            .collect()
    }

    pub fn unreview(&self, reviewed: &HunkSpan) -> Option<ReviewedVersion> {
        let open = changes(self.reviewed, self.current);
        let range = translate(
            open.iter().map(|change| (&change.after, &change.before)),
            &reviewed.new,
        )?;
        let reviewed_lines = Lines::new(self.reviewed);
        let base = Lines::new(self.base);
        let mut rebuild = Rebuild::on(self.attribution);
        rebuild.keep(&reviewed_lines, 0..range.start, self.attribution)?;
        for line in base.get(reviewed.old.clone())? {
            rebuild.push(line, &self.attribution.default);
        }
        rebuild.keep(
            &reviewed_lines,
            range.end..reviewed_lines.len(),
            self.attribution,
        )?;
        Some(self.version(rebuild.finish(self.base)))
    }

    pub(crate) fn version(&self, reviewed: Reviewed) -> ReviewedVersion {
        if reviewed.text == self.current {
            ReviewedVersion::Current(reviewed.attribution)
        } else if reviewed.text == self.base {
            ReviewedVersion::Base
        } else {
            ReviewedVersion::Partial(reviewed)
        }
    }
}

pub fn replay(old_base: &[u8], reviewed: &Reviewed, new_base: &[u8]) -> Reviewed {
    if old_base == new_base {
        return reviewed.clone();
    }
    let attribution = &reviewed.attribution;
    let upstream = changes(old_base, new_base);
    let reviewed_lines = Lines::new(&reviewed.text);
    let base_lines = Lines::new(new_base);
    let mut rebuild = Rebuild::with_default(attribution.default.clone());
    let mut position = 0;
    for change in changes(old_base, &reviewed.text) {
        let Some(target) = translate(
            upstream
                .iter()
                .map(|change| (&change.before, &change.after)),
            &change.before,
        )
        .filter(|target| target.start >= position) else {
            continue;
        };
        for line in base_lines.get(position..target.start).unwrap_or_default() {
            rebuild.push(line, &attribution.default);
        }
        for (old, new) in change.before.clone().zip(target.clone()) {
            rebuild.remove(new, attribution.removed_by(old));
        }
        for (line, text) in change
            .after
            .clone()
            .zip(reviewed_lines.get(change.after.clone()).unwrap_or_default())
        {
            rebuild.push(text, attribution.added_by(line));
        }
        position = target.end;
    }
    for line in base_lines.rest(position).unwrap_or_default() {
        rebuild.push(line, &attribution.default);
    }
    rebuild.finish(new_base)
}

#[cfg(test)]
#[path = "review.tests.rs"]
mod tests;
