use test_case::test_case;

use super::*;

#[test_case(&[("HERDR_SOCKET_PATH", "/run/herdr.sock"), ("HC_TERMINAL_SOCKET", "/run/hc.sock")], Some(HostKind::Herdr); "Herdr run in a terminal of herdr-app")]
#[test_case(&[("HC_TERMINAL_SOCKET", "/run/hc.sock")], Some(HostKind::HerdrCore); "herdr-app")]
#[test_case(&[("HERDR_SOCKET_PATH", ""), ("HC_TERMINAL_SOCKET", "/run/hc.sock")], Some(HostKind::HerdrCore); "an empty Herdr socket")]
#[test_case(&[("HC_TERMINAL_SOCKET", "")], None; "an empty herdr-core socket")]
fn the_host_is_the_one_whose_socket_the_pane_is_given(
    environment: &[(&str, &str)],
    expected: Option<HostKind>,
) {
    let var = |name: &str| {
        environment
            .iter()
            .find(|(set, _)| *set == name)
            .map(|(_, value)| OsString::from(value))
    };
    assert_eq!(host_kind(var), expected);
}

fn arguments(arguments: &[&str]) -> impl Iterator<Item = String> {
    arguments
        .iter()
        .map(|argument| (*argument).to_owned())
        .collect::<Vec<_>>()
        .into_iter()
}

#[test]
fn the_mod_names_the_terminal_focused_when_the_pane_opened() {
    assert_eq!(
        focused_terminal(arguments(&["--focused-terminal", "t3"])).unwrap(),
        Some(PaneId("t3".to_owned()))
    );
    assert_eq!(focused_terminal(arguments(&[])).unwrap(), None);
}

#[test_case(&["--focused-terminal"]; "without its terminal")]
#[test_case(&["t3"]; "a bare terminal")]
#[test_case(&["--focused-terminal", "t3", "--theme"]; "an unknown option after it")]
fn other_arguments_are_refused_with_the_usage(given: &[&str]) {
    let error = focused_terminal(arguments(given)).unwrap_err();
    assert!(error.to_string().starts_with("usage: reviewer"), "{error}");
}
