use std::collections::VecDeque;
use std::time::{Duration, Instant};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Widget};
use ui_frame::Frame;
use ui_theme::Palette;
use unicode_width::UnicodeWidthStr;
use uuid::Uuid;

const LONG_TOAST_DELAY: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ToastId(Uuid);

impl ToastId {
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Toast {
    text: String,
    expires: Instant,
    kind: ToastKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LongToast {
    id: ToastId,
    text: String,
    started: Instant,
}

#[derive(Debug)]
pub struct ToastState {
    toasts: Vec<Toast>,
    long_toasts: VecDeque<LongToast>,
    long_toast_delay: Duration,
}

impl Default for ToastState {
    fn default() -> Self {
        Self::with_long_toast_delay(LONG_TOAST_DELAY)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToastKind {
    Info,
    Error,
}

impl ToastKind {
    fn duration(self) -> Duration {
        match self {
            Self::Info => Duration::from_secs(3),
            Self::Error => Duration::from_secs(6),
        }
    }
}

impl ToastState {
    pub fn with_long_toast_delay(delay: Duration) -> Self {
        Self {
            toasts: Vec::new(),
            long_toasts: VecDeque::new(),
            long_toast_delay: delay,
        }
    }

    pub fn changes_between(&self, previous: Instant, now: Instant) -> bool {
        self.toasts.iter().any(|toast| toast.expires <= now)
            || self.long_toasts.front().is_some_and(|toast| {
                let appears = toast.started + self.long_toast_delay;
                previous < appears && appears <= now
            })
    }

    pub fn push(&mut self, text: impl Into<String>, kind: ToastKind) {
        self.push_at(text, kind, Instant::now());
    }

    fn push_at(&mut self, text: impl Into<String>, kind: ToastKind, now: Instant) {
        let text = text.into();
        self.toasts
            .retain(|toast| toast.text != text || toast.kind != kind);
        self.toasts.push(Toast {
            text,
            expires: now + kind.duration(),
            kind,
        });
    }

    pub fn expire(&mut self, now: Instant) {
        self.toasts.retain(|toast| toast.expires > now);
    }

    pub fn start_long_toast(&mut self, text: impl Into<String>) -> ToastId {
        self.start_long_toast_at(text, Instant::now())
    }

    fn start_long_toast_at(&mut self, text: impl Into<String>, now: Instant) -> ToastId {
        let id = ToastId::generate();
        self.long_toasts.push_back(LongToast {
            id,
            text: text.into(),
            started: now,
        });
        id
    }

    pub fn finish_toast(&mut self, id: ToastId) {
        if let Some(index) = self.long_toasts.iter().position(|toast| toast.id == id) {
            self.long_toasts.remove(index);
        }
    }

    pub fn render(&self, area: Rect, buffer: &mut Buffer, palette: Palette) {
        self.render_at(area, buffer, palette, Instant::now());
    }

    fn render_at(&self, area: Rect, buffer: &mut Buffer, palette: Palette, now: Instant) {
        let long_toast = self
            .long_toasts
            .front()
            .filter(|toast| now.saturating_duration_since(toast.started) >= self.long_toast_delay)
            .map(|toast| (toast.text.as_str(), ToastKind::Info));
        let short_toasts = self
            .toasts
            .iter()
            .rev()
            .map(|toast| (toast.text.as_str(), toast.kind));
        let mut bottom = area.bottom();
        for (text, kind) in long_toast.into_iter().chain(short_toasts) {
            let view = ToastView::new(text, kind, area.width, palette);
            let (width, height) = view.size();
            let width = width.min(area.width);
            if bottom.saturating_sub(area.y) < height {
                break;
            }
            bottom -= height;
            view.render(
                Rect::new(area.right().saturating_sub(width), bottom, width, height),
                buffer,
            );
        }
    }
}

const TOAST_MARGIN: u16 = 4;

struct ToastView {
    lines: Vec<String>,
    kind: ToastKind,
    palette: Palette,
}

impl ToastView {
    fn new(text: &str, kind: ToastKind, width: u16, palette: Palette) -> Self {
        Self {
            lines: wrap_words(text, usize::from(width.saturating_sub(TOAST_MARGIN))),
            kind,
            palette,
        }
    }

    fn size(&self) -> (u16, u16) {
        let text = self
            .lines
            .iter()
            .map(|line| line.width())
            .max()
            .unwrap_or(0);
        let clamp = |size: usize| u16::try_from(size).unwrap_or(u16::MAX);
        (
            clamp(text).saturating_add(TOAST_MARGIN),
            clamp(self.lines.len()).saturating_add(2),
        )
    }
}

impl Widget for ToastView {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        Clear.render(area, buffer);
        let color = match self.kind {
            ToastKind::Info => self.palette.focus,
            ToastKind::Error => self.palette.deletion,
        };
        let style = Style::default().fg(color).add_modifier(Modifier::BOLD);
        let lines: Vec<Line> = self
            .lines
            .into_iter()
            .map(|line| Line::from(Span::styled(line, style)))
            .collect();
        Paragraph::new(lines)
            .block(Frame::Notice(color).block(self.palette, ""))
            .render(area, buffer);
    }
}

fn wrap_words(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split(' ') {
        if !current.is_empty() && current.width() + 1 + word.width() > width {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    lines.push(current);
    lines
}

#[cfg(test)]
#[path = "lib.tests.rs"]
mod tests;
