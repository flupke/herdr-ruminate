use std::collections::HashMap;

use test_case::test_case;

use super::*;

/// The pane that the variables `set` name.
fn pane(set: &[(&str, &str)]) -> Option<HookPane> {
    let variables: HashMap<&str, &str> = set.iter().copied().collect();
    HookPane::from_vars(|name| variables.get(name).map(OsString::from))
}

const RUNTIME: (&str, &str) = ("XDG_RUNTIME_DIR", "/run/user/1");

#[test]
fn a_pane_of_herdr_reaches_the_reviewers_of_its_server() {
    let herdr = [
        RUNTIME,
        ("HERDR_SOCKET_PATH", "/run/herdr.sock"),
        ("HERDR_PANE_ID", "w1:p2"),
        // A Herdr started from a terminal of herdr-app passes that terminal's variables on.
        ("HC_TERMINAL_SOCKET", "/run/hc.sock"),
        ("HC_TERMINAL_ID", "t7"),
    ];

    assert_eq!(
        pane(&herdr),
        Some(HookPane {
            pane: PaneId("w1:p2".into()),
            directory: HookDirectory::for_server(
                Path::new("/run/user/1"),
                Path::new("/run/herdr.sock")
            ),
            core: None,
        })
    );
}

#[test]
fn a_terminal_of_herdr_core_reaches_the_reviewers_of_its_core_and_reports_there() {
    let core = [
        RUNTIME,
        ("HERDR_SOCKET_PATH", ""),
        ("HC_TERMINAL_SOCKET", "/run/hc.sock"),
        ("HC_TERMINAL_ID", "t7"),
    ];

    assert_eq!(
        pane(&core),
        Some(HookPane {
            pane: PaneId("t7".into()),
            directory: HookDirectory::for_server(
                Path::new("/run/user/1"),
                Path::new("/run/hc.sock")
            ),
            core: Some("/run/hc.sock".into()),
        })
    );
}

#[test_case(&[("HERDR_SOCKET_PATH", "/run/herdr.sock"), ("HERDR_PANE_ID", "w1:p2")]; "no runtime directory")]
#[test_case(&[RUNTIME, ("HERDR_SOCKET_PATH", "/run/herdr.sock"), ("HC_TERMINAL_ID", "t7")]; "Herdr's socket without its pane")]
#[test_case(&[RUNTIME, ("HC_TERMINAL_ID", "t7")]; "no host's socket")]
fn a_process_outside_a_pane_of_a_host_has_none(set: &[(&str, &str)]) {
    assert_eq!(pane(set), None);
}
