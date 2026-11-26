use std::cell::{Cell, RefCell};
use std::ops::Range;

use ansi_to_tui::IntoText;
use component_core::{AnyInput, Component, ComponentSubscriptions, EventPublisher, InputScope};
use linear_issues::LinearIssue;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use ui_actions::Action;
use ui_controls::{KeyHint, label_width};
use ui_events::{
    CommitMessageToggleRequested, FilesOverviewChanged, PointerInput, PointerInputKind,
    RepositoryMetadataChanged, RevisionSelectorRequested, SearchStatusChanged,
};
use ui_theme::Palette;
use unicode_width::UnicodeWidthStr;

const MIN_TERMINAL_WIDTH: u16 = 40;
const MIN_TERMINAL_HEIGHT: u16 = 6;
const LEADING: &str = " ";
const TITLE_GAP: &str = "  ";
const ISSUE_GAP: &str = "  ";
const MIN_TITLE_ROOM: usize = 32;
const FOOTER_GAP: &str = "  ";
const PROGRESS_BAR_CELLS: usize = 12;
const PROGRESS_BAR_MIN_WIDTH: u16 = 72;

struct HeaderColumns {
    id: Range<usize>,
    title: Range<usize>,
}

impl HeaderColumns {
    fn new(id_width: usize, title_width: usize, room: usize) -> Self {
        let id_start = LEADING.width().min(room);
        let title_start = (id_start + id_width + TITLE_GAP.width()).min(room);
        Self {
            id: id_start..(id_start + id_width).min(room),
            title: title_start..(title_start + title_width).min(room),
        }
    }
}

pub struct StatusComponent {
    events: EventPublisher,
    description: String,
    display_id: Line<'static>,
    overview: FilesOverviewChanged,
    search: SearchStatusChanged,
    issues: Vec<LinearIssue>,
    title_room: Cell<u16>,
    issue_links: RefCell<Vec<(Range<usize>, String)>>,
}

impl StatusComponent {
    pub fn new(events: EventPublisher) -> Self {
        Self {
            events,
            description: String::new(),
            display_id: Line::default(),
            overview: FilesOverviewChanged::default(),
            search: SearchStatusChanged::default(),
            issues: Vec::new(),
            title_room: Cell::new(0),
            issue_links: RefCell::new(Vec::new()),
        }
    }

    pub fn render_terminal_too_small(&self, area: Rect, buffer: &mut Buffer) -> bool {
        if area.width >= MIN_TERMINAL_WIDTH && area.height >= MIN_TERMINAL_HEIGHT {
            return false;
        }
        Paragraph::new("Terminal is too small\nMinimum: 40x6\nq quit").render(area, buffer);
        true
    }

    pub fn render_header(&self, area: Rect, buffer: &mut Buffer, palette: Palette) {
        let summary = self.summary(area.width, palette);
        let summary_width = u16::try_from(summary.width())
            .unwrap_or(u16::MAX)
            .min(area.width);
        let room = area.width.saturating_sub(summary_width.saturating_add(1));
        let issues = self.issue_columns_in(usize::from(room));
        let title_width = issues
            .first()
            .map_or(room, |first| clamp_u16(first.start - ISSUE_GAP.width()));
        for (issue, columns) in self.issues.iter().zip(&issues) {
            Paragraph::new(Span::styled(
                issue.id.clone(),
                Style::default()
                    .fg(palette.focus)
                    .add_modifier(Modifier::UNDERLINED),
            ))
            .render(
                Rect::new(
                    area.x.saturating_add(clamp_u16(columns.start)),
                    area.y,
                    clamp_u16(columns.len()),
                    1,
                ),
                buffer,
            );
        }
        self.issue_links.replace(
            issues
                .into_iter()
                .zip(&self.issues)
                .map(|(columns, issue)| (columns, issue.url.clone()))
                .collect(),
        );
        self.title_room.set(title_width);
        let mut title = Line::from(LEADING);
        title.spans.extend(self.display_id.spans.iter().cloned());
        title.spans.push(Span::raw(TITLE_GAP));
        title.spans.push(Span::styled(
            self.commit_title().to_owned(),
            Style::default().add_modifier(Modifier::BOLD),
        ));
        Paragraph::new(title)
            .style(Style::default().fg(palette.text))
            .render(Rect::new(area.x, area.y, title_width, 1), buffer);
        Paragraph::new(summary)
            .alignment(Alignment::Right)
            .style(Style::default().fg(palette.text))
            .render(
                Rect::new(
                    area.right().saturating_sub(summary_width),
                    area.y,
                    summary_width,
                    area.height,
                ),
                buffer,
            );
    }

    fn issue_columns_in(&self, room: usize) -> Vec<Range<usize>> {
        let title_needs = (LEADING.width()
            + self.display_id.width()
            + TITLE_GAP.width()
            + self.commit_title().width())
        .min(MIN_TITLE_ROOM);
        let gap = ISSUE_GAP.width();
        let mut shown = self.issues.len();
        while shown > 0 {
            let widths = self.issues[..shown].iter().map(|issue| issue.id.width());
            let issues_width = widths.clone().sum::<usize>() + gap * (shown - 1);
            let end = room.saturating_sub(gap - 1);
            if let Some(start) = end.checked_sub(issues_width)
                && start >= title_needs + gap
            {
                let mut columns = Vec::with_capacity(shown);
                let mut column = start;
                for width in widths {
                    columns.push(column..column + width);
                    column += width + gap;
                }
                return columns;
            }
            shown -= 1;
        }
        Vec::new()
    }

    fn summary(&self, width: u16, palette: Palette) -> Line<'static> {
        let FilesOverviewChanged {
            progress,
            lines_added,
            lines_removed,
        } = self.overview;
        let mut spans = vec![
            Span::styled(
                format!("+{lines_added}"),
                Style::default().fg(palette.insertion),
            ),
            Span::raw(" "),
            Span::styled(
                format!("-{lines_removed}"),
                Style::default().fg(palette.deletion),
            ),
            Span::raw("  "),
        ];
        if width >= PROGRESS_BAR_MIN_WIDTH {
            let done = progress.filled(PROGRESS_BAR_CELLS);
            spans.extend([
                Span::styled("━".repeat(done), Style::default().fg(palette.insertion)),
                Span::styled(
                    "━".repeat(PROGRESS_BAR_CELLS - done),
                    Style::default().fg(palette.border),
                ),
                Span::raw(" "),
            ]);
        }
        spans.push(Span::raw(format!("{}% reviewed ", progress.percent())));
        Line::from(spans)
    }

    pub fn render_footer(
        &self,
        area: Rect,
        buffer: &mut Buffer,
        palette: Palette,
        hints: &[KeyHint],
    ) {
        let Some(query) = &self.search.query else {
            let help = KeyHint::new("?", "help");
            let mut room = area.width.saturating_sub(help.width());
            let mut spans = Vec::new();
            for hint in hints {
                let width = hint.width().saturating_add(label_width(FOOTER_GAP));
                if width > room {
                    break;
                }
                room -= width;
                spans.extend(hint.spans(palette));
                spans.push(Span::raw(FOOTER_GAP));
            }
            spans.extend(help.spans(palette));
            Paragraph::new(Line::from(spans)).render(area, buffer);
            return;
        };
        let status = Line::raw(format!(
            "[{}/{}]",
            self.search.current_match, self.search.total_matches
        ));
        let width = u16::try_from(status.width())
            .unwrap_or(u16::MAX)
            .min(area.width);
        Paragraph::new(format!("/{query}"))
            .style(Style::default().fg(palette.text))
            .render(
                Rect::new(
                    area.x,
                    area.y,
                    area.width.saturating_sub(width.saturating_add(1)),
                    1,
                ),
                buffer,
            );
        Paragraph::new(status)
            .style(Style::default().fg(palette.text))
            .alignment(Alignment::Right)
            .render(
                Rect::new(area.right().saturating_sub(width), area.y, width, 1),
                buffer,
            );
    }

    fn repository_changed(&mut self, event: &RepositoryMetadataChanged) {
        self.description.clone_from(&event.description);
        self.issues.clone_from(&event.issues);
        self.display_id = event
            .display_id
            .as_bytes()
            .into_text()
            .ok()
            .and_then(|text| text.lines.into_iter().next())
            .unwrap_or_default();
    }

    fn overview_changed(&mut self, event: &FilesOverviewChanged) {
        self.overview = *event;
    }

    fn search_changed(&mut self, event: &SearchStatusChanged) {
        self.search.clone_from(event);
    }

    fn pointer_input(&mut self, input: PointerInput) -> Vec<Action> {
        if !matches!(input.kind, PointerInputKind::Click) {
            return Vec::new();
        }
        let Some(position) = input.position else {
            return Vec::new();
        };
        if position.terminal_row != 0 {
            return Vec::new();
        }
        let column = usize::from(position.terminal_column);
        if let Some((_, url)) = self
            .issue_links
            .borrow()
            .iter()
            .find(|(columns, _)| columns.contains(&column))
        {
            return vec![Action::OpenLink(url.clone())];
        }
        let columns = HeaderColumns::new(
            self.display_id.width(),
            self.commit_title().width(),
            usize::from(self.title_room.get()),
        );
        if columns.id.contains(&column) {
            self.events.publish(RevisionSelectorRequested);
        } else if columns.title.contains(&column) {
            self.events.publish(CommitMessageToggleRequested);
        }
        Vec::new()
    }

    fn commit_title(&self) -> &str {
        self.description
            .lines()
            .next()
            .unwrap_or("(no description set)")
    }
}

fn clamp_u16(columns: usize) -> u16 {
    u16::try_from(columns).unwrap_or(u16::MAX)
}

impl Component<Action> for StatusComponent {
    fn register_subscriptions(subscriptions: &mut ComponentSubscriptions<'_, Self, Action>) {
        subscriptions.subscribe(Self::repository_changed);
        subscriptions.subscribe(Self::overview_changed);
        subscriptions.subscribe(Self::search_changed);
        subscriptions.subscribe_input(InputScope::Hovered, AnyInput, Self::pointer_input);
    }
}

#[cfg(test)]
#[path = "lib.tests.rs"]
mod tests;
