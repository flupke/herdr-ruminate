use qrcode::{EcLevel, types::Color as Module};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};

const QUIET_ZONE: usize = 2;

const DARK: Color = Color::Rgb(0, 0, 0);
const LIGHT: Color = Color::Rgb(255, 255, 255);

pub struct QrCode {
    lines: Vec<Line<'static>>,
    width: u16,
}

impl QrCode {
    pub fn new(data: &str) -> Option<Self> {
        let code = qrcode::QrCode::with_error_correction_level(data, EcLevel::L).ok()?;
        let modules = code.width();
        let colors = code.to_colors();
        let size = modules + 2 * QUIET_ZONE;
        let dark = |column: usize, row: usize| {
            let (Some(column), Some(row)) = (
                column
                    .checked_sub(QUIET_ZONE)
                    .filter(|&column| column < modules),
                row.checked_sub(QUIET_ZONE).filter(|&row| row < modules),
            ) else {
                return false;
            };
            colors[row * modules + column] == Module::Dark
        };
        let color = |dark| if dark { DARK } else { LIGHT };
        let lines = (0..size)
            .step_by(2)
            .map(|row| {
                Line::from(
                    (0..size)
                        .map(|column| {
                            let style = Style::new()
                                .fg(color(dark(column, row)))
                                .bg(color(dark(column, row + 1)));
                            Span::styled("▀", style)
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        Some(Self {
            lines,
            width: u16::try_from(size).ok()?,
        })
    }

    pub fn width(&self) -> u16 {
        self.width
    }

    pub fn height(&self) -> u16 {
        u16::try_from(self.lines.len()).unwrap_or(u16::MAX)
    }

    pub fn text(&self) -> Text<'static> {
        Text::from(self.lines.clone())
    }
}

#[cfg(test)]
#[path = "lib.tests.rs"]
mod tests;
