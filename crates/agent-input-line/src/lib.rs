//! An agent's input line, read from its terminal's screen as herdr-core paints it to a viewer that
//! attaches: rows ended by CR LF, each character in the style its SGR escapes set. Claude Code's
//! input line is the box between two rules under the last `❯`; Codex's composer is the rows from
//! its last `›` down to the first blank row. Text there counts as typed unless it is drawn dim,
//! or, in Claude Code's box, in the style its `❯` is drawn in, as each agent draws a placeholder or a
//! suggested prompt. A screen that shows no input line, as one that shows a dialog, shows none:
//! nothing tells what its keys do. The rules are herdr-bowtie's `claude-input`, where the core's
//! paint carries a style across rows, as a terminal does, rather than starting each row anew.

use std::str::Chars;

/// The character Claude Code starts its input line with.
const CLAUDE_PROMPT: char = '❯';
/// The character Codex starts its composer with.
const CODEX_PROMPT: char = '›';
/// The character of the rules above and below Claude Code's input line.
const RULE: char = '─';

/// The agent whose input line a screen shows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Agent {
    ClaudeCode,
    Codex,
}

/// What an agent's input line shows.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Line {
    /// Nothing typed: blank, or only a placeholder or a suggested prompt.
    Empty,
    /// Typed text: the typed characters, the line's rows joined by `\n`, without the blanks
    /// that start and end it.
    Typed(String),
    /// The screen shows no input line, as while a dialog shows.
    Missing,
}

/// What the input line of `agent` shows on `screen`, the paint of its terminal.
pub fn read(agent: Agent, screen: &str) -> Line {
    let rows = rows(screen);
    let input = match agent {
        Agent::ClaudeCode => claude_input(&rows),
        Agent::Codex => codex_input(&rows),
    };
    let Some((placeholder, cells)) = input else {
        return Line::Missing;
    };
    let typed: String = cells
        .iter()
        .map(|cell| {
            if cell.text == '\n' || !(cell.style.dim || Some(cell.style) == placeholder) {
                cell.text
            } else {
                ' '
            }
        })
        .map(|text| if text == '\u{a0}' { ' ' } else { text })
        .collect();
    let typed = typed.trim();
    if typed.is_empty() {
        Line::Empty
    } else {
        Line::Typed(typed.to_owned())
    }
}

/// The cells of Claude Code's input line: the box between two rules, under the last `❯`, from
/// after the `❯` on, each row of the box ended by a `\n`; and the `❯`'s style when it is not
/// the terminal's own, which Claude Code may draw a placeholder in.
fn claude_input(rows: &[Vec<Cell>]) -> Option<(Option<Style>, Vec<Cell>)> {
    let start = rows
        .iter()
        .rposition(|row| first_visible(row).is_some_and(|cell| cell.text == CLAUDE_PROMPT))?;
    if start == 0 || !is_rule(&rows[start - 1]) {
        return None;
    }
    let end = rows[start + 1..].iter().position(|row| is_rule(row))?;
    let (placeholder, cells) = after_prompt(&rows[start..=start + end], CLAUDE_PROMPT)?;
    // A `❯` in the terminal's own style tells nothing from the user's text.
    Some((
        placeholder.filter(|style| *style != Style::default()),
        cells,
    ))
}

/// The cells of Codex's composer: from after its last `›` to the first blank row under it, each
/// row ended by a `\n`. Codex draws its placeholder dim, and its `›` in a style of its own.
fn codex_input(rows: &[Vec<Cell>]) -> Option<(Option<Style>, Vec<Cell>)> {
    let start = rows
        .iter()
        .rposition(|row| first_visible(row).is_some_and(|cell| cell.text == CODEX_PROMPT))?;
    let end = rows[start + 1..]
        .iter()
        .position(|row| first_visible(row).is_none())
        .map_or(rows.len() - 1, |blank| start + blank);
    let (_, cells) = after_prompt(&rows[start..=end], CODEX_PROMPT)?;
    Some((None, cells))
}

/// The cells of `rows` from after the `prompt` that starts the first one, each row ended by a
/// `\n`, and the style the `prompt` is drawn in.
fn after_prompt(rows: &[Vec<Cell>], prompt: char) -> Option<(Option<Style>, Vec<Cell>)> {
    let (first, rest) = rows.split_first()?;
    let glyph = first.iter().position(|cell| cell.text == prompt)?;
    let newline = Cell {
        text: '\n',
        style: Style::default(),
    };
    let mut cells = first[glyph + 1..].to_vec();
    for row in rest {
        cells.push(newline);
        cells.extend_from_slice(row);
    }
    Some((Some(first[glyph].style), cells))
}

/// One character on the screen, and how it is drawn.
#[derive(Clone, Copy, Debug)]
struct Cell {
    text: char,
    style: Style,
}

/// What of a character's drawing tells typed text from a placeholder.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Style {
    dim: bool,
    /// The foreground colour's SGR parameters; empty for the terminal's default.
    foreground: Colour,
}

/// A foreground colour: the SGR parameters that set it, up to the five of `38;2;r;g;b`.
type Colour = [u16; 5];

/// The rows of `screen`, each the characters it shows with the style each is drawn in. A style
/// set on one row holds on the next, as in a terminal.
fn rows(screen: &str) -> Vec<Vec<Cell>> {
    let mut style = Style::default();
    let mut rows = vec![Vec::new()];
    let mut chars = screen.chars();
    while let Some(text) = chars.next() {
        match text {
            '\u{1b}' => escape(&mut chars, &mut style),
            '\n' => rows.push(Vec::new()),
            text if text.is_control() => {}
            text => rows
                .last_mut()
                .expect("one row at least")
                .push(Cell { text, style }),
        }
    }
    rows
}

/// Reads the escape sequence that follows an ESC in `chars`, and applies it to `style` when it
/// is an SGR. An OSC runs to a BEL or to the ESC of its `ESC \`; a character set's designation
/// takes one more character.
fn escape(chars: &mut Chars<'_>, style: &mut Style) {
    match chars.next() {
        Some('[') => {
            let (params, end) = csi(chars);
            if end == Some('m') {
                apply_sgr(style, &params);
            }
        }
        Some(']') => {
            if chars.find(|next| *next == '\u{7}' || *next == '\u{1b}') == Some('\u{1b}') {
                chars.next();
            }
        }
        Some('(' | ')' | '*' | '+') => {
            chars.next();
        }
        _ => {}
    }
}

/// The parameters of the CSI sequence that follows its `ESC [` in `chars`, and its final
/// character, which says what it does: `m` for an SGR.
fn csi(chars: &mut Chars<'_>) -> (String, Option<char>) {
    let mut params = String::new();
    for next in chars.by_ref() {
        if ('@'..='~').contains(&next) {
            return (params, Some(next));
        }
        params.push(next);
    }
    (params, None)
}

/// Applies the SGR parameters `params` to `style`. An SGR with a private marker, as `>4;2m`,
/// sets no style.
fn apply_sgr(style: &mut Style, params: &str) {
    if params.starts_with(['<', '=', '>', '?']) {
        return;
    }
    let numbers: Vec<u16> = params
        .split([';', ':'])
        .map(|param| param.parse().unwrap_or(0))
        .collect();
    let mut rest = numbers.as_slice();
    while let Some((&number, tail)) = rest.split_first() {
        rest = tail;
        match number {
            0 => *style = Style::default(),
            2 => style.dim = true,
            22 => style.dim = false,
            30..=37 | 90..=97 => style.foreground = [number, 0, 0, 0, 0],
            39 => style.foreground = Colour::default(),
            // 48 takes a colour too, for the background.
            38 | 48 => {
                let (colour, used) = extended_colour(number, rest);
                if number == 38 {
                    style.foreground = colour;
                }
                rest = &rest[used..];
            }
            _ => {}
        }
    }
}

/// The colour of the SGR `code`, 38 or 48, whose parameters after it are `rest`: `2;r;g;b` or
/// `5;n`; and how many of `rest` it takes.
fn extended_colour(code: u16, rest: &[u16]) -> (Colour, usize) {
    let used = match rest.first() {
        Some(2) => 4,
        Some(5) => 2,
        _ => 0,
    }
    .min(rest.len());
    let mut colour = Colour::default();
    colour[0] = code;
    for (slot, value) in colour[1..].iter_mut().zip(&rest[..used]) {
        *slot = *value;
    }
    (colour, used)
}

/// The first character of `row` that is not blank.
fn first_visible(row: &[Cell]) -> Option<&Cell> {
    row.iter()
        .find(|cell| !cell.text.is_whitespace() && cell.text != '\u{a0}')
}

/// Whether `row` is a rule: drawn with `─` from its first visible character.
fn is_rule(row: &[Cell]) -> bool {
    first_visible(row).is_some_and(|cell| cell.text == RULE)
}

#[cfg(test)]
#[path = "lib.tests.rs"]
mod tests;
