use std::ops::{Range, RangeInclusive};

const EVIDENCE_CONTEXT_ROWS: usize = 3;

pub trait Layout {
    fn row_count(&self) -> usize;

    fn cursor_row(&self, position: &Position) -> usize;

    fn first_row_of(&self, row: usize) -> Option<usize>;

    fn nearest_cursor(&self, position: &Position, rows: Range<usize>) -> Option<(usize, usize)>;
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Position {
    cursor: usize,
    column: usize,
    scroll: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScreenAnchor {
    visual_row: usize,
    scroll: usize,
}

impl Position {
    pub fn new(cursor: usize, column: usize, scroll: usize) -> Self {
        Self {
            cursor,
            column,
            scroll,
        }
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn column(&self) -> usize {
        self.column
    }

    pub fn scroll(&self) -> usize {
        self.scroll
    }

    pub fn top(&self, row_count: usize) -> usize {
        self.scroll.min(row_count.saturating_sub(1))
    }

    pub fn move_to(&mut self, row: usize, rows: usize) {
        self.cursor = row.min(rows.saturating_sub(1));
    }

    pub fn set_column(&mut self, column: usize) {
        self.column = column;
    }

    pub fn restore(&mut self, saved: Self, rows: usize) {
        let last = rows.saturating_sub(1);
        self.cursor = saved.cursor.min(last);
        self.scroll = saved.scroll.min(last);
        self.column = saved.column;
    }

    pub fn restore_filling_screen(&mut self, saved: Self, rows: usize, height: usize) {
        self.restore(saved, rows);
        self.scroll = self.scroll.min(rows.saturating_sub(height));
    }

    pub fn keep_cursor_visible(&mut self, layout: &impl Layout, height: usize) {
        let mut scroll = self.scroll.min(last_top(layout, height));
        let cursor = layout.cursor_row(self);
        if cursor < scroll {
            scroll = cursor;
        } else if cursor >= scroll.saturating_add(height) {
            scroll = cursor + 1 - height;
        }
        self.scroll = scroll;
    }

    pub fn center_cursor(&mut self, layout: &impl Layout, height: usize) {
        self.scroll = layout
            .cursor_row(self)
            .saturating_sub(height / 2)
            .min(last_top(layout, height));
    }

    pub fn align_cursor_top(&mut self, layout: &impl Layout) {
        self.scroll = layout
            .first_row_of(self.cursor)
            .unwrap_or_else(|| layout.cursor_row(self));
    }

    pub fn scroll_by(&mut self, delta: isize, layout: &impl Layout, height: usize) -> bool {
        self.scroll = self
            .scroll
            .saturating_add_signed(delta)
            .min(last_top(layout, height));
        self.contain_cursor(layout, height)
    }

    pub fn contain_cursor(&mut self, layout: &impl Layout, height: usize) -> bool {
        let row_count = layout.row_count();
        let cursor = layout.cursor_row(self);
        let top = self.top(row_count);
        let visible = top..top.saturating_add(height).min(row_count);
        if visible.contains(&cursor) {
            return false;
        }
        let Some((row, column)) = layout.nearest_cursor(self, visible) else {
            return false;
        };
        self.cursor = row;
        self.column = column;
        true
    }

    pub fn screen_anchor(&self, layout: &impl Layout) -> ScreenAnchor {
        ScreenAnchor {
            visual_row: layout.cursor_row(self),
            scroll: self.scroll,
        }
    }

    pub fn return_to(&mut self, anchor: ScreenAnchor, layout: &impl Layout) {
        self.scroll = anchor
            .scroll
            .saturating_add(layout.cursor_row(self))
            .saturating_sub(anchor.visual_row);
    }

    pub fn pin(&mut self, row: usize, screen_row: usize, layout: &impl Layout, height: usize) {
        self.scroll = row.saturating_sub(screen_row).min(last_top(layout, height));
    }

    pub fn reveal_comment(
        &mut self,
        rows: RangeInclusive<usize>,
        editing: bool,
        layout: &impl Layout,
        height: usize,
    ) {
        let (start, end) = rows.into_inner();
        if editing {
            let bottom = end.saturating_add(1).saturating_sub(height);
            self.scroll = if bottom <= start {
                self.scroll.clamp(bottom, start)
            } else {
                bottom
            };
        } else if start < self.scroll || end >= self.scroll.saturating_add(height) {
            self.scroll = start.saturating_sub(1).min(last_top(layout, height));
        }
    }

    pub fn reveal_evidence(&mut self, rows: Range<usize>, row_count: usize, height: usize) {
        let before = height.saturating_sub(rows.len()).min(EVIDENCE_CONTEXT_ROWS);
        self.scroll = rows
            .start
            .saturating_sub(before)
            .min(row_count.saturating_sub(height));
    }
}

fn last_top(layout: &impl Layout, height: usize) -> usize {
    layout.row_count().saturating_sub(height)
}

#[cfg(test)]
#[path = "lib.tests.rs"]
mod tests;
