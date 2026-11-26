use agent_input_line::Agent;
use herdr_client::protocol::PromptWait;
use test_case::test_case;

use super::{Reader, is_pasted, reports_working, wait_for};

fn claude_line(line: &str) -> String {
    format!("────\r\n❯\u{a0}{line}\r\n────\r\n")
}

#[test_case(&claude_line(""), None; "an empty line")]
#[test_case(&claude_line("\u{1b}[2mTry \"fix it\"\u{1b}[0m"), None; "a placeholder")]
#[test_case(&claude_line("fix the"), Some(&PromptWait::TypedText); "typed text")]
#[test_case("❯ 1. Yes\r\n  2. No\r\n", Some(&PromptWait::NoInputLine); "a dialog")]
fn a_prompt_to_claude_code_waits_for_what_its_line_shows(screen: &str, wait: Option<&PromptWait>) {
    assert_eq!(
        wait_for(&Reader::Reads(Agent::ClaudeCode), screen).as_ref(),
        wait
    );
}

#[test]
fn a_prompt_to_an_agent_whose_line_is_not_read_waits() {
    assert_eq!(
        wait_for(&Reader::Unread("pi".to_owned()), &claude_line("")),
        Some(PromptWait::UnreadAgent("pi".to_owned()))
    );
}

#[test_case("\u{1b}]7501;state=working:app=claude-code\u{1b}\\", true; "working, ended by ESC backslash")]
#[test_case("\u{1b}]7501;app=claude-code:state=working\u{7}", true; "working, ended by BEL")]
#[test_case("\u{1b}]7501;state=working:app=claude-code\u{7}❯\u{1b}]7501;state=idle:app=claude-code\u{7}", false; "the last report, idle")]
#[test_case("\u{1b}]7501;state=idle:app=claude-code:msg=working\u{7}", false; "a message that says working")]
#[test_case("❯ working", false; "no report")]
fn the_agent_reports_working_by_its_last_report_on_the_screen(screen: &str, working: bool) {
    assert_eq!(reports_working(screen), working);
}

#[test_case("Review comments for you in review r1", true; "the prompt whole")]
#[test_case("Review comments for", true; "a long start, as herdr-core pastes it")]
#[test_case("Review comments for\n  you", true; "a long start wrapped otherwise")]
#[test_case("[Pasted text #1 +12 lines]", true; "Claude Code's stand-in for a long paste")]
#[test_case("[Pasted Content 1200 chars]", true; "Codex's stand-in for a long paste")]
#[test_case("R", false; "a person's first key, as the prompt starts")]
#[test_case("Rev", false; "a person's first keys, as the prompt starts")]
#[test_case("Run the tests", false; "someone's text")]
#[test_case("Review comments for you and more", false; "the prompt with someone's text after it")]
fn text_in_the_line_is_the_pasted_prompt_only_when_long_enough(shown: &str, pasted: bool) {
    assert_eq!(
        is_pasted(shown, "Review comments for you in review r1"),
        pasted
    );
}

#[test]
fn a_short_prompt_is_pasted_only_whole() {
    assert!(is_pasted("/resume s1", "/resume s1"));
    assert!(!is_pasted("/res", "/resume s1"));
}
