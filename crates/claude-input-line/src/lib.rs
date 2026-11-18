//! Claude Code's input line, read from the text of its terminal's screen, as herdr-core's
//! `terminal.read` gives it, without styles: the box between two rules, under the last `❯`.
//! Without styles, a placeholder or a suggested prompt there reads as typed text, so the line
//! reads empty only when it shows nothing. A screen that shows no such box, as a dialog, reads
//! as not empty: nothing tells what its keys do. The same rules as herdr-bowtie's
//! `claude-input`, which also reads the styles Herdr 0.9 gives.

/// The character Claude Code starts its input line with.
const PROMPT: char = '❯';
/// The character of the rules above and below the input line.
const RULE: char = '─';

/// Whether `screen`, the text of a Claude Code terminal's screen, shows its input line with
/// nothing in it.
pub fn input_is_empty(screen: &str) -> bool {
    input_text(screen).is_some_and(|text| text.chars().all(char::is_whitespace))
}

/// The text of the input line on `screen`, from after its `❯` to the rule under it; `None` when
/// the screen shows no input line.
fn input_text(screen: &str) -> Option<String> {
    let lines: Vec<&str> = screen.split('\n').collect();
    let start = lines
        .iter()
        .rposition(|line| line.trim_start().starts_with(PROMPT))?;
    if start == 0 || !is_rule(lines[start - 1]) {
        return None;
    }
    let end = lines[start + 1..].iter().position(|line| is_rule(line))?;
    let (_, first) = lines[start].split_once(PROMPT)?;
    let mut text = first.to_owned();
    for next in &lines[start + 1..=start + end] {
        text.push('\n');
        text.push_str(next);
    }
    Some(text)
}

/// Whether `line` is a rule: drawn with `─` from its first visible character.
fn is_rule(line: &str) -> bool {
    line.trim_start().starts_with(RULE)
}

#[cfg(test)]
mod tests {
    use test_case::test_case;

    use super::input_is_empty;

    /// The bottom of Claude Code 2.1.3xx's screen in vim mode, its input line showing `line`,
    /// as herdr-core's `terminal.read` gives it: the top rule carries the session's name.
    fn screen(line: &str) -> String {
        format!(
            "● Done.\n\n──────── rayon · worker ─\n❯ {line}\n────────────────────────\n  -- \
             INSERT -- ⏵⏵ auto mode on (shift+tab to cycle)\n"
        )
    }

    #[test_case(&screen(""), true; "an empty line")]
    #[test_case(&screen("\u{a0}\u{a0}"), true; "a line of non-breaking spaces")]
    #[test_case(&screen("fix the"), false; "typed text")]
    #[test_case(&screen("Try \"refactor x.rs\""), false; "a placeholder, which reads as typed without styles")]
    #[test_case(&screen("first\n  second"), false; "text on a second line of the box")]
    #[test_case(&screen("\n  second"), false; "text on the second line alone")]
    #[test_case("● Done.\n❯ 1. Yes\n  2. No\n", false; "a dialog's cursor with no box")]
    #[test_case("────\n❯ \n", false; "a box with no rule under it")]
    #[test_case("", false; "an empty screen")]
    fn the_input_line_reads_empty_only_when_its_box_shows_nothing(screen: &str, empty: bool) {
        assert_eq!(input_is_empty(screen), empty);
    }

    #[test]
    fn the_last_input_line_on_the_screen_is_the_one_read() {
        // A prompt of the transcript above the box starts with `❯` too.
        let screen = format!("────\n❯ an earlier prompt\n────\n{}", screen(""));

        assert!(input_is_empty(&screen));
    }

    #[test]
    fn a_dialog_s_cursor_under_the_box_is_no_input_line() {
        let screen = format!("{}❯ 1. Yes\n  2. No\n", screen(""));

        assert!(!input_is_empty(&screen));
    }
}
