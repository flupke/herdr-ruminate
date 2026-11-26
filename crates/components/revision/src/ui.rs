use std::collections::HashMap;

use ansi_to_tui::IntoText;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};
use review_repository::repository::{
    ChangeId, RevisionCandidate, RevisionDirection, RevisionHistoryLine, RevisionRef,
    RevisionRefKind,
};
use ui_frame::{Frame, with_footer};
use ui_shortcuts::Key;
use ui_theme::Palette;
use unicode_width::UnicodeWidthStr;

const REFS_GAP: &str = "  ";
const CHIP_GAP: &str = " ";
const CHIP_PADDING: usize = 2;
const MIN_SHORTENED_NAME: usize = 3;
const CROWDED_NAME_WIDTH: usize = 20;
const REFS_ROOM_DIVISOR: usize = 3;
const MAX_REFS_FROM_DESCRIPTION: usize = 30;
const MIN_DESCRIPTION_AFTER_ID: usize = 2;

pub(super) enum RevisionShare {
    Percent(u64),
    NothingToReview,
    Unreadable(String),
}

pub(super) struct RevisionHistorySelection {
    lines: Vec<RevisionHistoryLine>,
    selected_line: usize,
    filter: Option<RevisionHistoryFilter>,
    reviewed: HashMap<ChangeId, RevisionShare>,
}

#[derive(Default)]
struct RevisionHistoryFilter {
    query: String,
}

pub(super) enum RevisionHistoryInputResult {
    None,
    Close,
    Edit(ChangeId),
}

pub(super) struct SelectableRevision {
    direction: RevisionDirection,
    candidate: RevisionCandidate,
}

impl RevisionHistoryFilter {
    fn match_priority(&self, line: &RevisionHistoryLine) -> Option<u8> {
        if line
            .short_change_id
            .as_ref()
            .is_some_and(|short_change_id| {
                starts_with_ignore_ascii_case(short_change_id, &self.query)
            })
        {
            return Some(0);
        }
        (fuzzy_matches(&line.plain_text, &self.query)
            || line
                .refs
                .iter()
                .any(|reference| fuzzy_matches(&reference.name, &self.query)))
        .then_some(1)
    }
}

fn starts_with_ignore_ascii_case(text: &str, prefix: &str) -> bool {
    text.get(..prefix.len())
        .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
}

fn fuzzy_matches(text: &str, query: &str) -> bool {
    let mut query = query.chars();
    let Some(mut expected) = query.next() else {
        return true;
    };
    for character in text.chars() {
        if character.eq_ignore_ascii_case(&expected) {
            let Some(next) = query.next() else {
                return true;
            };
            expected = next;
        }
    }
    false
}

impl RevisionHistorySelection {
    pub(super) fn new(lines: Vec<RevisionHistoryLine>) -> Self {
        let selected_line = lines
            .iter()
            .position(|line| line.is_current)
            .or_else(|| lines.iter().position(|line| line.change_id.is_some()))
            .unwrap_or_default();
        Self {
            lines,
            selected_line,
            filter: None,
            reviewed: HashMap::new(),
        }
    }

    pub(super) fn reviewed(&mut self, change_id: ChangeId, share: RevisionShare) {
        self.reviewed.insert(change_id, share);
    }

    fn share_of(&self, line: &RevisionHistoryLine) -> Option<&RevisionShare> {
        self.reviewed.get(line.change_id.as_ref()?)
    }

    pub(super) fn row_count(&self) -> usize {
        self.lines.len()
    }

    pub(super) fn handle_key(&mut self, key: Key) -> RevisionHistoryInputResult {
        if self.filter.is_some() {
            self.handle_filter_key(key)
        } else {
            self.handle_navigation_key(key)
        }
    }

    fn match_priority(&self, line: &RevisionHistoryLine) -> Option<u8> {
        if line.is_immutable || line.change_id.is_none() {
            return None;
        }
        self.filter
            .as_ref()
            .map_or(Some(0), |filter| filter.match_priority(line))
    }

    fn best_matching_line(&self) -> Option<usize> {
        self.lines
            .iter()
            .enumerate()
            .filter_map(|(index, line)| self.match_priority(line).map(|priority| (priority, index)))
            .min()
            .map(|(_, index)| index)
    }

    fn move_next(&mut self) {
        if let Some(next) = self
            .lines
            .iter()
            .enumerate()
            .skip(self.selected_line.saturating_add(1))
            .find_map(|(index, line)| self.is_selectable(line).then_some(index))
        {
            self.selected_line = next;
        }
    }

    fn move_previous(&mut self) {
        if let Some(previous) = self.lines[..self.selected_line.min(self.lines.len())]
            .iter()
            .rposition(|line| self.is_selectable(line))
        {
            self.selected_line = previous;
        }
    }

    fn start_filter(&mut self) {
        self.filter = Some(RevisionHistoryFilter::default());
    }

    fn update_filter(&mut self, character: Option<char>) {
        let Some(filter) = &mut self.filter else {
            return;
        };
        if let Some(character) = character {
            filter.query.push(character.to_ascii_lowercase());
        } else {
            filter.query.pop();
        }
        if filter.query.is_empty() {
            return;
        }
        if let Some(matching_line) = self.best_matching_line() {
            self.selected_line = matching_line;
        }
    }

    fn stop_filter(&mut self) {
        self.filter = None;
    }

    fn query(&self) -> Option<&str> {
        self.filter.as_ref().map(|filter| filter.query.as_str())
    }

    fn is_selectable(&self, line: &RevisionHistoryLine) -> bool {
        self.match_priority(line).is_some()
    }

    fn selected_change_id(&self) -> Option<ChangeId> {
        self.lines
            .get(self.selected_line)
            .filter(|line| self.is_selectable(line))
            .and_then(|line| line.change_id.clone())
    }

    fn handle_navigation_key(&mut self, key: Key) -> RevisionHistoryInputResult {
        match key {
            Key::Char('/') => self.start_filter(),
            Key::Char('j') | Key::Down => self.move_next(),
            Key::Char('k') | Key::Up => self.move_previous(),
            Key::Escape => return RevisionHistoryInputResult::Close,
            Key::Enter => return self.edit_result(),
            _ => {}
        }
        RevisionHistoryInputResult::None
    }

    fn handle_filter_key(&mut self, key: Key) -> RevisionHistoryInputResult {
        match key {
            Key::Down => self.move_next(),
            Key::Up => self.move_previous(),
            Key::Backspace => self.update_filter(None),
            Key::Char(character) if character.is_ascii_alphanumeric() => {
                self.update_filter(Some(character));
            }
            Key::Escape => self.stop_filter(),
            Key::Enter => return self.edit_result(),
            _ => {}
        }
        RevisionHistoryInputResult::None
    }

    fn edit_result(&self) -> RevisionHistoryInputResult {
        self.selected_change_id().map_or(
            RevisionHistoryInputResult::None,
            RevisionHistoryInputResult::Edit,
        )
    }
}

impl SelectableRevision {
    pub(super) const fn new(direction: RevisionDirection, candidate: RevisionCandidate) -> Self {
        Self {
            direction,
            candidate,
        }
    }

    pub(super) fn candidate(&self) -> &RevisionCandidate {
        &self.candidate
    }
}

struct SelectorPopup {
    area: Rect,
    inner: Rect,
    footer: Option<Line<'static>>,
}

impl SelectorPopup {
    fn new(terminal_area: Rect, row_count: usize) -> Self {
        let width = terminal_area.width.saturating_mul(4) / 5;
        let height = u16::try_from(row_count.max(1))
            .unwrap_or(u16::MAX)
            .saturating_add(2)
            .min(terminal_area.height);
        let area = Rect::new(
            terminal_area.x + terminal_area.width.saturating_sub(width) / 2,
            terminal_area.y + terminal_area.height.saturating_sub(height) / 2,
            width,
            height,
        );
        Self {
            area,
            inner: Block::default().borders(Borders::ALL).inner(area),
            footer: None,
        }
    }

    fn with_footer(self, footer: Option<Line<'static>>) -> Self {
        Self { footer, ..self }
    }

    fn render(
        &self,
        buffer: &mut Buffer,
        title: impl Into<ratatui::text::Line<'static>>,
        palette: &Palette,
        lines: Vec<Line<'_>>,
        viewport_line: usize,
        highlighted_line: Option<usize>,
    ) {
        Clear.render(self.area, buffer);
        let block = Frame::Popup.block(*palette, title);
        match self.footer.clone() {
            Some(footer) => with_footer(block, footer),
            None => block,
        }
        .render(self.area, buffer);
        let visible_rows = usize::from(self.inner.height).max(1);
        let scroll = viewport_line.saturating_add(1).saturating_sub(visible_rows);
        Paragraph::new(lines)
            .scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0))
            .render(self.inner, buffer);
        if let Some(highlighted_line) = highlighted_line {
            let visible_line = highlighted_line.saturating_sub(scroll);
            if visible_line < visible_rows {
                let row = self.inner.y + u16::try_from(visible_line).unwrap_or(self.inner.height);
                for column in self.inner.left()..self.inner.right() {
                    if let Some(cell) = buffer.cell_mut((column, row))
                        && cell.bg != palette.cursor
                    {
                        cell.set_bg(palette.selection);
                    }
                }
            }
        }
    }
}

pub(super) fn selector_area(terminal_area: Rect, row_count: usize) -> Rect {
    SelectorPopup::new(terminal_area, row_count).area
}

pub(super) fn render_loading(area: Rect, buffer: &mut Buffer, palette: &Palette) {
    SelectorPopup::new(area, 1).render(
        buffer,
        "Select revision",
        palette,
        vec![Line::raw("Loading revision history…")],
        0,
        None,
    );
}

pub(super) fn render_candidates(
    area: Rect,
    buffer: &mut Buffer,
    palette: &Palette,
    candidates: &[SelectableRevision],
    selected: usize,
) {
    let lines = candidates
        .iter()
        .map(|candidate| {
            let description = if candidate.candidate.description.is_empty() {
                "(no description set)"
            } else {
                &candidate.candidate.description
            };
            let relation = match candidate.direction {
                RevisionDirection::Parents => "parent",
                RevisionDirection::Children => "child ",
            };
            Line::raw(format!(
                "{relation}  {}  {description}",
                candidate.candidate.short_change_id
            ))
        })
        .collect::<Vec<_>>();
    let lines = if lines.is_empty() {
        vec![Line::from("No parent or child revisions")]
    } else {
        lines
    };
    SelectorPopup::new(area, candidates.len()).render(
        buffer,
        "Select revision",
        palette,
        lines,
        selected,
        Some(selected),
    );
}

pub(super) fn render_history(
    area: Rect,
    buffer: &mut Buffer,
    palette: &Palette,
    selection: &RevisionHistorySelection,
) {
    let title = match selection.query() {
        None => "Select revision".to_owned(),
        Some(query) => format!("Select revision: /{query}"),
    };
    let popup = SelectorPopup::new(area, selection.lines.len());
    let width = usize::from(popup.inner.width);
    let lines = selection
        .lines
        .iter()
        .map(|history_line| history_row(history_line, selection, palette, width))
        .collect::<Vec<_>>();
    let selected = selection
        .lines
        .get(selection.selected_line)
        .filter(|line| selection.is_selectable(line));
    let reason = selected
        .and_then(|line| match selection.share_of(line) {
            Some(RevisionShare::Unreadable(reason)) => Some(reason),
            _ => None,
        })
        .map(|reason| Line::styled(format!("? {reason}"), Style::default().fg(palette.warning)));
    popup.with_footer(reason).render(
        buffer,
        title,
        palette,
        lines,
        selection.selected_line,
        selected.map(|_| selection.selected_line),
    );
}

fn history_row(
    line: &RevisionHistoryLine,
    selection: &RevisionHistorySelection,
    palette: &Palette,
    width: usize,
) -> Line<'static> {
    let Some(end) = line
        .graph_end
        .filter(|end| line.text.is_char_boundary(*end))
    else {
        return ansi_line(&line.text).unwrap_or_else(|| Line::raw(line.plain_text.clone()));
    };
    let (Some(graph), Some(revision)) = (
        ansi_line(line.text[..end].trim_end_matches(' ')),
        ansi_line(&line.text[end..]),
    ) else {
        return Line::raw(line.plain_text.clone());
    };
    let mark = match (line.is_immutable, selection.share_of(line)) {
        (true, _) => String::new(),
        (false, Some(RevisionShare::Percent(percent))) => format!("{percent}%"),
        (false, Some(RevisionShare::NothingToReview)) => "–".to_owned(),
        (false, Some(RevisionShare::Unreadable(_))) => "?".to_owned(),
        (false, None) => "·".to_owned(),
    };
    let mut spans = graph.spans;
    spans.push(Span::styled(
        format!(" {mark:>4} "),
        Style::default().fg(palette.dim),
    ));
    let room = width.saturating_sub(spans.iter().map(Span::width).sum());
    let revision_width = revision.width();
    let id_width = line
        .short_change_id
        .as_deref()
        .map_or(0, |id| id.width() + MIN_DESCRIPTION_AFTER_ID);
    let refs_room = room
        .saturating_sub(revision_width + REFS_GAP.width())
        .max((room / REFS_ROOM_DIVISOR).min(MAX_REFS_FROM_DESCRIPTION))
        .min(room.saturating_sub(id_width + REFS_GAP.width()));
    let chips = ref_chips(&line.refs, refs_room, palette);
    if chips.is_empty() {
        spans.extend(revision.spans);
        return Line::from(spans);
    }
    let chips_width = chips.iter().map(Span::width).sum::<usize>();
    spans.extend(shorten_spans(
        revision.spans,
        room.saturating_sub(chips_width + REFS_GAP.width()),
    ));
    spans.push(Span::raw(REFS_GAP));
    spans.extend(chips);
    // Herdr paints the blank cells that end a row in the style of the cell before them, and
    // ratatui writes a cell only when it differs from the blank cell under it.
    spans.push(Span::styled(" ", Style::default().fg(palette.text)));
    Line::from(spans)
}

fn ref_chips(refs: &[RevisionRef], room: usize, palette: &Palette) -> Vec<Span<'static>> {
    let full_width = refs
        .iter()
        .map(|reference| reference.name.width() + CHIP_PADDING)
        .sum::<usize>()
        + CHIP_GAP.width() * refs.len().saturating_sub(1);
    let name_width = if full_width <= room {
        usize::MAX
    } else {
        CROWDED_NAME_WIDTH
    };
    let mut chips = Chips {
        spans: Vec::new(),
        used: 0,
        room,
    };
    for (index, reference) in refs.iter().enumerate() {
        let Some(name) = chips.fitting_name(&reference.name, name_width, refs.len() - index - 1)
        else {
            chips.push_if_fits(
                hidden_count(refs.len() - index),
                Style::default().fg(palette.dim),
            );
            break;
        };
        let color = match reference.kind {
            RevisionRefKind::Bookmark => palette.focus,
            RevisionRefKind::Tag => palette.warning,
        };
        chips.push_if_fits(
            format!(" {name} "),
            Style::default().fg(color).bg(palette.cursor),
        );
    }
    chips.spans
}

struct Chips {
    spans: Vec<Span<'static>>,
    used: usize,
    room: usize,
}

impl Chips {
    fn gap(&self) -> usize {
        if self.spans.is_empty() {
            0
        } else {
            CHIP_GAP.width()
        }
    }

    fn fitting_name(&self, name: &str, name_width: usize, after: usize) -> Option<String> {
        let count_width = if after == 0 {
            0
        } else {
            CHIP_GAP.width() + hidden_count(after).width()
        };
        let name_room = self
            .room
            .saturating_sub(self.used + self.gap() + CHIP_PADDING + count_width);
        let name = ui_panes::shorten(name, name_width);
        if name.width() <= name_room {
            return Some(name);
        }
        (self.spans.is_empty() && name_room >= MIN_SHORTENED_NAME)
            .then(|| ui_panes::shorten(&name, name_room))
            .filter(|name| name.width() <= name_room)
    }

    fn push_if_fits(&mut self, text: String, style: Style) {
        let width = self.gap() + text.width();
        if self.used + width > self.room {
            return;
        }
        if !self.spans.is_empty() {
            self.spans.push(Span::raw(CHIP_GAP));
        }
        self.used += width;
        self.spans.push(Span::styled(text, style));
    }
}

fn hidden_count(count: usize) -> String {
    format!("+{count}")
}

fn shorten_spans(spans: Vec<Span<'static>>, width: usize) -> Vec<Span<'static>> {
    if spans.iter().map(Span::width).sum::<usize>() <= width {
        return spans;
    }
    let mut shortened = Vec::new();
    let mut used = 0;
    let room = width.saturating_sub(1);
    for span in spans {
        let span_width = span.width();
        if used + span_width <= room {
            used += span_width;
            shortened.push(span);
            continue;
        }
        // unicode-width gives an emoji's variation selector, as in `❤️`, no width on its own: ratatui's
        // buffer measures the string whole.
        let mut content = String::new();
        for character in span.content.chars() {
            content.push(character);
            if used + content.width() > room {
                content.pop();
                break;
            }
        }
        if width > 0 {
            content.push('…');
        }
        shortened.push(Span::styled(content, span.style));
        break;
    }
    shortened
}

fn ansi_line(text: &str) -> Option<Line<'static>> {
    if text.is_empty() {
        return Some(Line::default());
    }
    text.as_bytes()
        .into_text()
        .ok()
        .and_then(|text| text.lines.into_iter().next())
}
