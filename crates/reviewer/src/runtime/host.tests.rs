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
fn the_mod_names_the_terminal_focused_when_the_pane_opened_and_its_window() {
    assert_eq!(
        Arguments::parse(arguments(&[
            "--focused-terminal",
            "t3",
            "--herdr-app-home",
            "/home/u/.config/herdr-app"
        ]))
        .unwrap(),
        Arguments {
            focused_terminal: Some(PaneId("t3".to_owned())),
            herdr_app_home: Some("/home/u/.config/herdr-app".into()),
        }
    );
    assert_eq!(
        Arguments::parse(arguments(&[
            "--herdr-app-home",
            "/h",
            "--focused-terminal",
            "t3"
        ]))
        .unwrap()
        .focused_terminal,
        Some(PaneId("t3".to_owned()))
    );
    assert_eq!(
        Arguments::parse(arguments(&[])).unwrap(),
        Arguments::default()
    );
}

#[test_case(&["--focused-terminal"]; "without its terminal")]
#[test_case(&["--focused-terminal", "t3", "--herdr-app-home"]; "without its folder")]
#[test_case(&["t3"]; "a bare terminal")]
#[test_case(&["--focused-terminal", "t3", "--theme"]; "an unknown option after it")]
fn other_arguments_are_refused_with_the_usage(given: &[&str]) {
    let error = Arguments::parse(arguments(given)).unwrap_err();
    assert!(error.to_string().starts_with("usage: reviewer"), "{error}");
}

#[test_case(Some("/tmp/app"), Some("/home/u"), Some("/tmp/app"); "the window the mod names")]
#[test_case(None, Some("/home/u"), Some("/home/u/.config/herdr-app"); "herdr-app's default home")]
#[test_case(None, Some(""), None; "an empty HOME")]
#[test_case(None, None, None; "no HOME")]
fn the_pane_follows_the_focus_of_the_window_the_mod_names_else_the_default_one(
    named: Option<&str>,
    home: Option<&str>,
    expected: Option<&str>,
) {
    let var = |name: &str| (name == "HOME").then(|| home.map(OsString::from)).flatten();
    assert_eq!(
        focus_home(named.map(PathBuf::from), var),
        expected.map(|path| AppHome::new(path.into()))
    );
}
