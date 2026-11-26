use unicode_width::UnicodeWidthStr;

use crate::{DiffControl, LoadedDocument};

const MIN_DIFF_CONTROLS_WIDTH: u16 = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TitleControls(&'static [(&'static str, DiffControl)]);

impl TitleControls {
    const DIFF: Self = Self(&[
        (" ←→ ", DiffControl::ExpandAll),
        (" →← ", DiffControl::ContractAll),
        (" 👁  ", DiffControl::ShowFile),
    ]);
    const BASIC_DIFF: Self = Self(&[
        (" ←→ ", DiffControl::ExpandAll),
        (" →← ", DiffControl::ContractAll),
    ]);
    const FILE: Self = Self(&[(" ✕ ", DiffControl::CloseFile)]);

    pub(super) fn shown(file: &LoadedDocument, width: u16) -> Option<Self> {
        if file.temporary {
            return None;
        }
        let diff = &file.document.diff;
        let (controls, minimum) = if diff.is_file_view() {
            (Self::FILE, Self::FILE.width_with_corner())
        } else if diff.can_show_file() {
            (Self::DIFF, MIN_DIFF_CONTROLS_WIDTH)
        } else {
            (Self::BASIC_DIFF, MIN_DIFF_CONTROLS_WIDTH)
        };
        (width >= minimum).then_some(controls)
    }

    pub(super) fn title(self) -> String {
        self.0.iter().map(|(label, _)| *label).collect()
    }

    pub(super) fn width(self) -> usize {
        self.0.iter().map(|(label, _)| label.width()).sum()
    }

    pub(super) fn at(self, width: u16, column: u16) -> Option<DiffControl> {
        let start = usize::from(width).checked_sub(self.width() + 1)?;
        let mut offset = usize::from(column).checked_sub(start)?;
        for (label, control) in self.0 {
            if offset < label.width() {
                return Some(*control);
            }
            offset -= label.width();
        }
        None
    }

    fn width_with_corner(self) -> u16 {
        u16::try_from(self.width() + 2).unwrap_or(u16::MAX)
    }
}

#[cfg(test)]
#[path = "title_controls.tests.rs"]
mod tests;
