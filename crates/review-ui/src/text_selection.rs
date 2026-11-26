use std::cell::RefCell;

use ratatui::layout::Position;
use ui_text_selection::{TextLayer, TextPress, TextSelection};

use crate::{Action, TerminalAction, UserInput};

#[derive(Default)]
pub(crate) struct ScreenText {
    layer: RefCell<TextLayer>,
    selection: Option<Selecting>,
    size: Option<(u16, u16)>,
}

enum Selecting {
    AltDragging(Option<TextSelection>),
    PlainDragging(TextSelection),
    Copied(TextSelection),
}

pub(crate) enum TextInput {
    Taken(Vec<Action>),
    Passed(Vec<Action>),
}

impl ScreenText {
    pub(crate) fn set_layer(&self, layer: TextLayer) {
        self.layer.replace(layer);
    }

    pub(crate) fn shown(&self) -> Option<&TextSelection> {
        match self.selection.as_ref()? {
            Selecting::AltDragging(selection) => selection.as_ref(),
            Selecting::PlainDragging(selection) | Selecting::Copied(selection) => Some(selection),
        }
    }

    pub(crate) fn input(&mut self, message: &UserInput) -> TextInput {
        if let UserInput::Resize { width, height } = message {
            self.resize((*width, *height));
            return TextInput::Passed(Vec::new());
        }
        match (message, self.selection.take()) {
            (UserInput::MouseAltClick { column, row }, _) => {
                let selection = self.select(*column, *row, TextPress::Alt);
                self.selection = Some(Selecting::AltDragging(selection));
                TextInput::Taken(Vec::new())
            }
            (UserInput::MouseClick { column, row }, _) => {
                self.selection = self
                    .select(*column, *row, TextPress::Plain)
                    .map(Selecting::PlainDragging);
                TextInput::Passed(Vec::new())
            }
            (UserInput::MouseDrag { column, row }, Some(mut dragging)) if dragging.is_drag() => {
                if let Selecting::AltDragging(Some(selection))
                | Selecting::PlainDragging(selection) = &mut dragging
                {
                    selection.extend(Position::new(*column, *row));
                }
                self.selection = Some(dragging);
                TextInput::Taken(Vec::new())
            }
            (UserInput::MouseRelease, Some(Selecting::AltDragging(selection))) => {
                TextInput::Taken(self.release(selection))
            }
            (UserInput::MouseRelease, Some(Selecting::PlainDragging(selection))) => {
                TextInput::Passed(self.release(Some(selection)))
            }
            (_, Some(dragging)) if dragging.is_drag() => {
                self.selection = Some(dragging);
                TextInput::Passed(Vec::new())
            }
            _ => TextInput::Passed(Vec::new()),
        }
    }

    fn resize(&mut self, size: (u16, u16)) {
        let redraw = self.size.replace(size) == Some(size);
        if !redraw && matches!(self.selection, Some(Selecting::Copied(_))) {
            self.selection = None;
        }
    }

    fn release(&mut self, selection: Option<TextSelection>) -> Vec<Action> {
        let Some((selection, text)) =
            selection.and_then(|selection| selection.text().map(|text| (selection, text)))
        else {
            return Vec::new();
        };
        self.selection = Some(Selecting::Copied(selection));
        vec![Action::Terminal(TerminalAction::CopyToClipboard(text))]
    }

    fn select(&self, column: u16, row: u16, press: TextPress) -> Option<TextSelection> {
        self.layer
            .borrow()
            .select(Position::new(column, row), press)
    }
}

impl Selecting {
    fn is_drag(&self) -> bool {
        !matches!(self, Self::Copied(_))
    }
}
