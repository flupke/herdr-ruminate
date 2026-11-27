use std::str::Chars;

const CLAUDE_PROMPT: char = '❯';
const CODEX_PROMPT: char = '›';
const ANTIGRAVITY_PROMPT: char = '>';
const RULE: char = '─';

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Agent {
    ClaudeCode,
    Codex,
    Antigravity,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Line {
    Empty,
    Typed(String),
    Missing,
}

pub fn read(agent: Agent, screen: &str) -> Line {
    let screen = parse_screen(screen);
    let input = match agent {
        Agent::ClaudeCode => claude_input(&screen.rows),
        Agent::Codex => codex_input(&screen.rows),
        Agent::Antigravity => antigravity_input(&screen),
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

// Claude Code may draw a placeholder in the style of its `❯`.
fn claude_input(rows: &[Vec<Cell>]) -> Option<(Option<Style>, Vec<Cell>)> {
    let (start, end) = ruled_box(rows, CLAUDE_PROMPT)?;
    let (placeholder, cells) = after_prompt(&rows[start..end], CLAUDE_PROMPT)?;
    Some((
        placeholder.filter(|style| *style != Style::default()),
        cells,
    ))
}

// Codex draws its placeholder dim, and its `›` in a style of its own.
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

// Antigravity draws its placeholder in the style of its top rule, and hides the terminal cursor
// (`ESC [ ? 25 l`) while a dialog or panel takes keys (agy 1.5.18).
fn antigravity_input(screen: &Screen) -> Option<(Option<Style>, Vec<Cell>)> {
    if screen.cursor_hidden {
        return None;
    }
    let (start, end) = ruled_box(&screen.rows, ANTIGRAVITY_PROMPT)?;
    let (_, cells) = after_prompt(&screen.rows[start..end], ANTIGRAVITY_PROMPT)?;
    let placeholder = first_visible(&screen.rows[start - 1])
        .map(|cell| cell.style)
        .filter(|style| *style != Style::default());
    Some((placeholder, cells))
}

fn ruled_box(rows: &[Vec<Cell>], prompt: char) -> Option<(usize, usize)> {
    let start = rows.iter().enumerate().rposition(|(index, row)| {
        index > 0
            && is_rule(&rows[index - 1])
            && first_visible(row).is_some_and(|cell| cell.text == prompt)
    })?;
    let end = start + 1 + rows[start + 1..].iter().position(|row| is_rule(row))?;
    if rows[end + 1..]
        .iter()
        .any(|row| first_visible(row).is_some_and(|cell| cell.text == prompt))
    {
        return None;
    }
    Some((start, end))
}

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

#[derive(Clone, Copy, Debug)]
struct Cell {
    text: char,
    style: Style,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Style {
    dim: bool,
    foreground: Colour,
}

type Colour = [u16; 5];

struct Screen {
    rows: Vec<Vec<Cell>>,
    cursor_hidden: bool,
}

// In a terminal, as in herdr-core's paint, a style set on one row holds on the next.
fn parse_screen(screen: &str) -> Screen {
    let mut style = Style::default();
    let mut cursor_hidden = false;
    let mut rows = vec![Vec::new()];
    let mut chars = screen.chars();
    while let Some(text) = chars.next() {
        match text {
            '\u{1b}' => escape(&mut chars, &mut style, &mut cursor_hidden),
            '\n' => rows.push(Vec::new()),
            text if text.is_control() => {}
            text => rows
                .last_mut()
                .expect("one row at least")
                .push(Cell { text, style }),
        }
    }
    Screen {
        rows,
        cursor_hidden,
    }
}

// In a terminal, an OSC runs to a BEL or to the ESC of its `ESC \`; a character set's
// designation takes one more character.
fn escape(chars: &mut Chars<'_>, style: &mut Style, cursor_hidden: &mut bool) {
    match chars.next() {
        Some('[') => {
            let (params, end) = csi(chars);
            apply_csi(&params, end, style, cursor_hidden);
        }
        Some(']') => {
            if chars.find(|next| *next == '\u{7}' || *next == '\u{1b}') == Some('\u{1b}') {
                chars.next();
            }
        }
        Some('(' | ')' | '*' | '+') => {
            chars.next();
        }
        Some('c') => {
            *style = Style::default();
            *cursor_hidden = false;
        }
        _ => {}
    }
}

fn apply_csi(params: &str, end: Option<char>, style: &mut Style, cursor_hidden: &mut bool) {
    match end {
        Some('m') => apply_sgr(style, params),
        Some('l') if has_private_mode(params, "25") => *cursor_hidden = true,
        Some('h') if has_private_mode(params, "25") => *cursor_hidden = false,
        Some('p') if params == "!" => {
            *style = Style::default();
            *cursor_hidden = false;
        }
        _ => {}
    }
}

fn has_private_mode(params: &str, mode: &str) -> bool {
    params
        .strip_prefix('?')
        .is_some_and(|modes| modes.split(';').any(|item| item == mode))
}

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

// A terminal's SGR with a private marker, as `>4;2m`, sets no style.
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
            // SGR 48 takes a colour too, for the background.
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

// SGR 38 and 48 take `2;r;g;b` or `5;n`.
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

fn first_visible(row: &[Cell]) -> Option<&Cell> {
    row.iter()
        .find(|cell| !cell.text.is_whitespace() && cell.text != '\u{a0}')
}

fn is_rule(row: &[Cell]) -> bool {
    first_visible(row).is_some_and(|cell| cell.text == RULE)
}

#[cfg(test)]
#[path = "lib.tests.rs"]
mod tests;
