use test_case::test_case;

use super::{Agent, Line, read};

fn claude_screen(line: &str) -> String {
    format!(
        "● Done.\r\n\r\n\u{1b}[38;2;136;136;136m──────── rayon · worker ─\u{1b}[0m\r\n❯\u{a0}\u{1b}[0m{line}\u{1b}[0m\r\n\
         \u{1b}[38;2;136;136;136m────────────────────────\u{1b}[0m\r\n  -- INSERT -- ⏵⏵ auto mode on"
    )
}

fn codex_screen(line: &str) -> String {
    format!(
        "  /review - review any changes\r\n\r\n\u{1b}[1m\u{1b}[48;2;65;69;76m›\u{1b}[0m\u{1b}[48;2;65;69;76m \u{1b}[0m{line}\u{1b}[0m\r\n\
         \r\n  gpt-6.1-sol medium · ~/src · Ready"
    )
}

fn dim(text: &str) -> String {
    format!("\u{1b}[2m{text}\u{1b}[0m")
}

fn typed(text: &str) -> Line {
    Line::Typed(text.to_owned())
}

#[test_case(&claude_screen(""), &Line::Empty; "an empty line")]
#[test_case(&claude_screen("\u{a0}\u{a0}"), &Line::Empty; "a line of non-breaking spaces")]
#[test_case(&claude_screen(&dim("Try \"refactor x.rs\"")), &Line::Empty; "a placeholder, drawn dim")]
#[test_case(&claude_screen("fix the"), &typed("fix the"); "typed text")]
#[test_case(&claude_screen(&format!("fix {}", dim("the tests"))), &typed("fix"); "typed text before a dim suggestion")]
#[test_case(&claude_screen("first\r\n  second"), &typed("first\n  second"); "text on two rows of the box")]
#[test_case(&claude_screen("first\r\n❯ quoted"), &typed("first\n❯ quoted"); "multi-line text with a continuation row starting with prompt")]
#[test_case(&claude_screen("\r\n  second"), &typed("second"); "text on the second row alone")]
#[test_case("● Done.\r\n❯ 1. Yes\r\n  2. No\r\n", &Line::Missing; "a dialog's cursor with no box")]
#[test_case(&format!("{}\r\n❯ 1. Yes\r\n  2. No\r\n", claude_screen("")), &Line::Missing; "a dialog cursor below the box")]
#[test_case("────\r\n❯ \r\n", &Line::Missing; "a box with no rule under it")]
#[test_case("", &Line::Missing; "an empty screen")]
fn claude_code_s_line_reads_typed_only_for_text_drawn_as_typed(screen: &str, line: &Line) {
    assert_eq!(&read(Agent::ClaudeCode, screen), line);
}

#[test]
fn a_style_set_on_one_row_holds_on_the_next() {
    let screen = "────\u{1b}[2m\r\n❯ Try this\r\n────\r\n";

    assert_eq!(read(Agent::ClaudeCode, screen), Line::Empty);
}

#[test]
fn text_in_the_style_of_claude_code_s_prompt_is_a_placeholder() {
    let grey = "\u{1b}[38;2;153;153;153m";
    let screen = format!("────\r\n{grey}❯ Try this\u{1b}[0m\r\n────\r\n");

    assert_eq!(read(Agent::ClaudeCode, &screen), Line::Empty);
}

#[test]
fn the_last_input_line_on_the_screen_is_the_one_read() {
    let screen = format!(
        "────\r\n❯ an earlier prompt\r\n────\r\n{}",
        claude_screen("")
    );

    assert_eq!(read(Agent::ClaudeCode, &screen), Line::Empty);
}

#[test]
fn escapes_that_set_no_style_leave_the_text_as_it_is() {
    let screen =
        claude_screen("\u{1b}]2;Claude Code\u{1b}\\\u{1b}(Bfix\u{1b}[118;3H\u{1b}[>4;2m it");

    assert_eq!(read(Agent::ClaudeCode, &screen), typed("fix it"));
}

#[test_case(&codex_screen(&dim("Ask Codex to do anything")), &Line::Empty; "its placeholder, drawn dim")]
#[test_case(&codex_screen(""), &Line::Empty; "an empty composer")]
#[test_case(&codex_screen("fix it"), &typed("fix it"); "typed text")]
#[test_case(&codex_screen("fix it\r\n  and test"), &typed("fix it\n  and test"); "text on two rows")]
#[test_case("› 1. Update now\r\n  2. Skip\r\n", &typed("1. Update now\n  2. Skip"); "a dialog's cursor, read as typed text")]
#[test_case("  To get started, describe a task\r\n", &Line::Missing; "no composer")]
fn codex_s_composer_reads_typed_only_for_text_drawn_as_typed(screen: &str, line: &Line) {
    assert_eq!(&read(Agent::Codex, screen), line);
}

fn antigravity_screen(line: &str) -> String {
    format!(
        "\u{1b}[0m\u{1b}[38;5;8m────────────────────────\u{1b}[0m\r\n\
         \u{1b}[0m\u{1b}[38;5;12m> \u{1b}[0m{line}\u{1b}[0m\r\n\
         \u{1b}[0m\u{1b}[38;5;8m────────────────────────\u{1b}[0m\r\n\
         \u{1b}[0m\u{1b}[38;5;8m? for shortcuts\u{1b}[0m  \u{1b}[38;5;3mauto\u{1b}[0m\u{1b}[2m · Gemini 4 Neon · high\u{1b}[0m"
    )
}

#[test_case(&antigravity_screen(""), &Line::Empty; "an empty line")]
#[test_case(&antigravity_screen("\u{1b}[38;5;8mAuto mode: edits & safe commands auto-approved\u{1b}[0m"), &Line::Empty; "a placeholder in the rule's style")]
#[test_case(&antigravity_screen(&dim("Type a message")), &Line::Empty; "a dim placeholder")]
#[test_case(&antigravity_screen("fix it"), &typed("fix it"); "typed text")]
#[test_case(&antigravity_screen("fix it\r\n  more"), &typed("fix it\n  more"); "text on two rows of the box")]
#[test_case(&antigravity_screen("first\r\n> quoted"), &typed("first\n> quoted"); "multi-line text with a continuation row starting with >")]
#[test_case("\u{1b}[0m\u{1b}[38;5;12m> Yes, I trust this folder\u{1b}[0m\r\n  No, exit\r\n", &Line::Missing; "a trust dialog with no box")]
#[test_case(&format!("\u{1b}[?25l{}", antigravity_screen("")), &Line::Missing; "a panel hiding the cursor above an empty box")]
#[test_case(&format!("\u{1b}[?2026;25l{}", antigravity_screen("")), &Line::Missing; "compound private mode hiding the cursor")]
#[test_case(&format!("\u{1b}[?25l\u{1b}[?25h{}", antigravity_screen("")), &Line::Empty; "a cursor hidden then shown again")]
#[test_case(&format!("\u{1b}[?25l\u{1b}c{}", antigravity_screen("")), &Line::Empty; "RIS resets a hidden cursor")]
#[test_case(&format!("{}\r\nSwitch Model\r\n\u{1b}[38;5;12m> Gemini 4 Neon\u{1b}[0m\r\n", antigravity_screen("")), &Line::Missing; "a menu whose cursor sits below the box")]
#[test_case("────\r\n> \r\n", &Line::Missing; "a box with no rule under it")]
#[test_case("", &Line::Missing; "an empty screen")]
fn antigravity_s_line_reads_typed_only_for_text_drawn_as_typed(screen: &str, line: &Line) {
    assert_eq!(&read(Agent::Antigravity, screen), line);
}

#[test]
fn antigravity_s_last_input_line_ignores_earlier_transcript_prompts() {
    let screen = format!(
        "\u{1b}[38;5;8m────────\u{1b}[0m\r\n\u{1b}[1m\u{1b}[38;5;4m> earlier prompt\u{1b}[0m\r\n  FIG\r\n{}",
        antigravity_screen("")
    );

    assert_eq!(read(Agent::Antigravity, &screen), Line::Empty);
}

fn painted(name: &str) -> String {
    let path = format!("{}/testdata/{name}.paint", env!("CARGO_MANIFEST_DIR"));
    String::from_utf8(std::fs::read(path).unwrap()).unwrap()
}

#[test_case(Agent::ClaudeCode, "claude-code-placeholder", &Line::Empty; "Claude Code 2.1.329's first placeholder")]
#[test_case(Agent::ClaudeCode, "claude-code-typed", &typed("fix it"); "Claude Code 2.1.329 with text typed")]
#[test_case(Agent::Codex, "codex-placeholder", &Line::Empty; "Codex 0.166's placeholder")]
#[test_case(Agent::Codex, "codex-typed", &typed("fix it"); "Codex 0.166 with text typed")]
#[test_case(Agent::Antigravity, "antigravity-placeholder", &Line::Empty; "Antigravity CLI 1.5.18's first placeholder")]
#[test_case(Agent::Antigravity, "antigravity-typed", &typed("fix it"); "Antigravity CLI 1.5.18 with text typed")]
#[test_case(Agent::Antigravity, "antigravity-help", &Line::Missing; "Antigravity CLI 1.5.18 with /help open")]
#[test_case(Agent::Antigravity, "antigravity-pasted", &typed("[Pasted text #1 +19 lines]"); "Antigravity CLI 1.5.18 with a long paste")]
fn a_screen_herdr_core_painted_reads_as_the_agent_showed_it(
    agent: Agent,
    paint: &str,
    line: &Line,
) {
    assert_eq!(&read(agent, &painted(paint)), line);
}
