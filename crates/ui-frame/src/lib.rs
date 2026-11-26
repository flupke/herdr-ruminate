use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType};
use ui_theme::Palette;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Frame {
    Pane { focused: bool },
    Popup,
    Card { selected: bool },
    Notice(Color),
}

impl Frame {
    pub fn block<'a>(self, palette: Palette, title: impl Into<Line<'a>>) -> Block<'a> {
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(self.border_style(palette));
        let title = title.into();
        if title.width() == 0 {
            return block;
        }
        let style = self.title_style(palette).patch(title.style);
        block.title(padded(title).style(style))
    }

    fn has_attention(self) -> bool {
        match self {
            Self::Pane { focused } => focused,
            Self::Card { selected } => selected,
            Self::Popup | Self::Notice(_) => true,
        }
    }

    pub fn border_style(self, palette: Palette) -> Style {
        Style::default().fg(match self {
            Self::Notice(color) => color,
            _ if self.has_attention() => palette.focus,
            _ => palette.border,
        })
    }

    fn title_style(self, palette: Palette) -> Style {
        match self {
            Self::Notice(color) => Style::default().fg(color).add_modifier(Modifier::BOLD),
            _ if self.has_attention() => Style::default()
                .fg(palette.text)
                .add_modifier(Modifier::BOLD),
            _ => Style::default().fg(palette.dim),
        }
    }
}

pub fn with_footer<'a>(block: Block<'a>, footer: impl Into<Line<'a>>) -> Block<'a> {
    let footer = footer.into();
    if footer.width() == 0 {
        return block;
    }
    block.title_bottom(padded(footer))
}

fn padded(line: Line<'_>) -> Line<'_> {
    let mut spans = Vec::with_capacity(line.spans.len() + 2);
    spans.push(Span::raw(" "));
    spans.extend(line.spans);
    spans.push(Span::raw(" "));
    let mut padded = Line::from(spans).style(line.style);
    padded.alignment = line.alignment;
    padded
}

#[cfg(test)]
#[path = "lib.tests.rs"]
mod tests;
