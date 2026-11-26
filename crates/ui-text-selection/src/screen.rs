use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use unicode_width::UnicodeWidthStr;

use crate::{RowEnd, TextCell, TextRegion, TextRow};

struct DrawnRow {
    y: u16,
    cells: Vec<(u16, TextCell)>,
    framed: bool,
    crosses_edge: bool,
}

impl TextRegion {
    pub fn screen(area: Rect, buffer: &Buffer) -> Self {
        let mut region = Self::new(area);
        region.push_screen(area, buffer);
        region
    }

    pub fn push_screen(&mut self, area: Rect, buffer: &Buffer) {
        for y in area.top()..area.bottom() {
            self.push_drawn(drawn_row(buffer, area, y), RowEnd::Line);
        }
    }

    pub fn push_prose(&mut self, area: Rect, buffer: &Buffer) {
        let rows = (area.top()..area.bottom())
            .filter_map(|y| prose_row(drawn_row(buffer, area, y)))
            .collect::<Vec<_>>();
        let mut continues = false;
        for (index, row) in rows.iter().enumerate() {
            let next = rows
                .get(index + 1)
                .filter(|next| next.y == row.y.saturating_add(1));
            let end = next.map_or(RowEnd::Line, |next| wrap_between(row, next, area));
            let cells = if continues {
                row.cells
                    .iter()
                    .skip_while(|(_, cell)| cell.is_blank())
                    .cloned()
                    .collect()
            } else {
                row.cells.clone()
            };
            self.push_drawn(
                DrawnRow {
                    y: row.y,
                    cells,
                    framed: row.framed,
                    crosses_edge: false,
                },
                end,
            );
            continues = matches!(end, RowEnd::Wrap { .. });
        }
    }

    fn push_drawn(&mut self, row: DrawnRow, end: RowEnd) {
        let x = row.cells.first().map_or(self.area.x, |(column, _)| *column);
        self.push(TextRow::new(
            Position::new(x, row.y),
            row.cells.into_iter().map(|(_, cell)| cell).collect(),
            end,
        ));
    }
}

fn drawn_row(buffer: &Buffer, area: Rect, y: u16) -> DrawnRow {
    let mut cells = Vec::new();
    let mut x = area.left();
    while x < area.right() {
        let Some(cell) = buffer.cell((x, y)) else {
            break;
        };
        let symbol = cell.symbol();
        let width = u16::try_from(symbol.width()).unwrap_or(1).max(1);
        cells.push((x, TextCell::new(symbol, width)));
        x = x.saturating_add(width);
    }
    let outside = |x: Option<u16>| {
        x.and_then(|x| buffer.cell((x, y)))
            .is_some_and(|cell| is_rule(&TextCell::new(cell.symbol(), 1)))
    };
    let mut row = DrawnRow {
        y,
        cells,
        framed: false,
        crosses_edge: outside(area.left().checked_sub(1)) || outside(Some(area.right())),
    };
    row.trim_end();
    row
}

impl DrawnRow {
    fn trim_end(&mut self) {
        while self.cells.last().is_some_and(|(_, cell)| cell.is_blank()) {
            self.cells.pop();
        }
    }

    fn first_word(&self) -> &[(u16, TextCell)] {
        let start = self
            .cells
            .iter()
            .position(|(_, cell)| !cell.is_blank())
            .unwrap_or(self.cells.len());
        let length = self.cells[start..]
            .iter()
            .position(|(_, cell)| cell.is_blank())
            .unwrap_or(self.cells.len() - start);
        &self.cells[start..start + length]
    }

    fn last_word(&self) -> &[(u16, TextCell)] {
        let start = self
            .cells
            .iter()
            .rposition(|(_, cell)| cell.is_blank())
            .map_or(0, |blank| blank + 1);
        &self.cells[start..]
    }
}

fn width(cells: &[(u16, TextCell)]) -> u16 {
    cells.iter().map(|(_, cell)| cell.width).sum()
}

fn prose_row(mut row: DrawnRow) -> Option<DrawnRow> {
    if row.crosses_edge
        || row
            .cells
            .iter()
            .find(|(_, cell)| !cell.is_blank())
            .is_some_and(|(_, cell)| is_rule(cell))
    {
        return None;
    }
    let first = row.cells.iter().position(|(_, cell)| !cell.is_blank());
    let last = row.cells.len().checked_sub(1);
    if let (Some(first), Some(last)) = (first, last)
        && first < last
        && row.cells[first].1.text == "│"
        && row.cells[last].1.text == "│"
    {
        let start = (first + 2).min(last);
        row.cells.truncate(last);
        row.cells.drain(..start);
        row.trim_end();
        row.framed = true;
    }
    Some(row)
}

fn wrap_between(row: &DrawnRow, next: &DrawnRow, area: Rect) -> RowEnd {
    let Some((column, cell)) = row.cells.last() else {
        return RowEnd::Line;
    };
    if row.framed || next.framed {
        return RowEnd::Line;
    }
    let word = next.first_word();
    let Some((_, first)) = word.first() else {
        return RowEnd::Line;
    };
    if is_box_drawing(first) || starts_item(next) {
        return RowEnd::Line;
    }
    let first_word = width(word);
    let used = column.saturating_add(cell.width).saturating_sub(area.x);
    if used.saturating_add(1).saturating_add(first_word) <= area.width {
        return RowEnd::Line;
    }
    let last_word = width(row.last_word());
    let cut = used == area.width && last_word.saturating_add(first_word) > area.width;
    RowEnd::Wrap { space: !cut }
}

fn starts_item(row: &DrawnRow) -> bool {
    let word = row
        .first_word()
        .iter()
        .map(|(_, cell)| cell.text.as_str())
        .collect::<String>();
    let numbered = word
        .strip_suffix(['.', ')'])
        .is_some_and(|number| !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()));
    numbered || ["•", "◦", "▪", "☐", "☑", "-", "*", "+"].contains(&word.as_str())
}

fn is_rule(cell: &TextCell) -> bool {
    is_box_drawing(cell) && !["│", "┃"].contains(&cell.text.as_str())
}

fn is_box_drawing(cell: &TextCell) -> bool {
    cell.text
        .chars()
        .all(|character| ('\u{2500}'..='\u{257f}').contains(&character))
}
