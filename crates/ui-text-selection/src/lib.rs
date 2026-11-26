mod screen;

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextCell {
    text: String,
    width: u16,
}

impl TextCell {
    pub fn new(text: impl Into<String>, width: u16) -> Self {
        Self {
            text: text.into(),
            width: width.max(1),
        }
    }

    fn is_blank(&self) -> bool {
        self.text.trim().is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowEnd {
    Line,
    Wrap { space: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextRow {
    position: Position,
    cells: Vec<TextCell>,
    end: RowEnd,
}

impl TextRow {
    pub fn new(position: Position, cells: Vec<TextCell>, end: RowEnd) -> Self {
        Self {
            position,
            cells,
            end,
        }
    }

    fn columns(&self) -> impl Iterator<Item = (u16, &TextCell)> {
        self.cells.iter().scan(self.position.x, |column, cell| {
            let start = *column;
            *column = column.saturating_add(cell.width);
            Some((start, cell))
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextPress {
    #[default]
    Plain,
    Alt,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextRegion {
    area: Rect,
    rows: Vec<TextRow>,
    needs: TextPress,
}

impl TextRegion {
    pub fn new(area: Rect) -> Self {
        Self {
            area,
            rows: Vec::new(),
            needs: TextPress::Plain,
        }
    }

    pub fn area(&self) -> Rect {
        self.area
    }

    #[must_use]
    pub fn needing_alt(self) -> Self {
        Self {
            needs: TextPress::Alt,
            ..self
        }
    }

    pub fn push(&mut self, row: TextRow) {
        if self.area.contains(row.position) {
            let index = self
                .rows
                .partition_point(|other| other.position.y <= row.position.y);
            self.rows.insert(index, row);
        }
    }

    #[must_use]
    pub fn placed(&self, area: Rect, place: impl Fn(Position) -> Option<Position>) -> Self {
        let mut placed = Self {
            needs: self.needs,
            ..Self::new(area)
        };
        for row in &self.rows {
            if let Some(position) = place(row.position) {
                placed.push(TextRow {
                    position,
                    ..row.clone()
                });
            }
        }
        placed
    }
}

#[derive(Clone, Debug, Default)]
pub struct TextLayer {
    regions: Vec<TextRegion>,
}

impl TextLayer {
    pub fn push(&mut self, region: TextRegion) {
        self.regions.push(region);
    }

    pub fn select(&self, position: Position, press: TextPress) -> Option<TextSelection> {
        let region = self
            .regions
            .iter()
            .rev()
            .find(|region| region.area.contains(position))?;
        if region.needs == TextPress::Alt && press == TextPress::Plain {
            return None;
        }
        Some(TextSelection {
            region: region.clone(),
            anchor: position,
            cursor: position,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextSelection {
    region: TextRegion,
    anchor: Position,
    cursor: Position,
}

impl TextSelection {
    pub fn extend(&mut self, position: Position) {
        let area = self.region.area;
        self.cursor = Position::new(
            position
                .x
                .clamp(area.x, area.right().saturating_sub(1).max(area.x)),
            position
                .y
                .clamp(area.y, area.bottom().saturating_sub(1).max(area.y)),
        );
    }

    pub fn text(&self) -> Option<String> {
        if self.anchor == self.cursor {
            return None;
        }
        let mut text = String::new();
        let mut previous: Option<&TextRow> = None;
        for (row, cells) in self.selected_rows() {
            if let Some(previous) = previous {
                text.push_str(match previous.end {
                    RowEnd::Wrap { space } if previous.position.y + 1 == row.position.y => {
                        if space { " " } else { "" }
                    }
                    RowEnd::Wrap { .. } | RowEnd::Line => "\n",
                });
            }
            for (_, cell) in cells {
                text.push_str(&cell.text);
            }
            previous = Some(row);
        }
        Some(text)
    }

    pub fn highlight(&self, buffer: &mut Buffer, style: Style) {
        if self.anchor == self.cursor {
            return;
        }
        for (row, cells) in self.selected_rows() {
            for (column, cell) in cells {
                for offset in 0..cell.width {
                    if let Some(painted) =
                        buffer.cell_mut((column.saturating_add(offset), row.position.y))
                    {
                        painted.set_style(style);
                    }
                }
            }
        }
    }

    fn ordered(&self) -> (Position, Position) {
        let key = |position: Position| (position.y, position.x);
        if key(self.anchor) <= key(self.cursor) {
            (self.anchor, self.cursor)
        } else {
            (self.cursor, self.anchor)
        }
    }

    fn selected_rows(&self) -> impl Iterator<Item = (&TextRow, Vec<(u16, &TextCell)>)> {
        let (start, end) = self.ordered();
        self.region
            .rows
            .iter()
            .filter(move |row| (start.y..=end.y).contains(&row.position.y))
            .map(move |row| {
                let y = row.position.y;
                let cells = row
                    .columns()
                    .filter(|(column, cell)| {
                        (y > start.y || column.saturating_add(cell.width) > start.x)
                            && (y < end.y || *column <= end.x)
                    })
                    .collect();
                (row, cells)
            })
    }
}

#[cfg(test)]
#[path = "lib.tests.rs"]
mod tests;
