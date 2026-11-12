use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use herdr_client::protocol::{TabId, WorkspaceId};
use review_explore_page::RoundPublisher;

use super::*;
use crate::PageHost;
use crate::tests::page_round;

/// The review pane in tab `tab` of the tests' workspace.
fn in_tab(tab: &str) -> PanePlace {
    PanePlace {
        workspace: WorkspaceId("w1".into()),
        tab: TabId(tab.into()),
    }
}

/// The review pane whose page the tests open.
fn place() -> PanePlace {
    in_tab("t1")
}

/// The page of the reviewer at `place`, recorded under `state`.
fn host_at(state: &Path, place: &PanePlace) -> PageHost {
    let round = RoundPublisher::default();
    PageHost::start(
        page_round(&round),
        &PageDirectory::new(state),
        place,
        Path::new("/repositories/drafts"),
    )
    .unwrap()
}

/// The page of the reviewer at [`place`], recorded under `state`.
fn host(state: &Path) -> PageHost {
    host_at(state, &place())
}

/// Opens the page of the review pane at `place` under `state` with `browser`.
fn opener_at(state: &Path, place: PanePlace, browser: Browser) -> PageOpener {
    PageOpener::new(PageDirectory::new(state), place, browser)
}

/// Opens the page of [`place`] under `state` with `browser`.
fn opener(state: &Path, browser: Browser) -> PageOpener {
    opener_at(state, place(), browser)
}

/// A browser that writes the address it opens to `address_file`, then exits with `status`.
fn browser(directory: &Path, address_file: &Path, status: u8) -> Browser {
    let script = directory.join("browser");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf %s \"$2\" > '{}'\nexit {status}\n",
            address_file.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    Browser::command(&format!("{} --new-tab", script.display()))
}

#[test]
fn the_opener_opens_the_page_of_its_tabs_reviewer_in_the_browser() {
    let state = tempfile::tempdir().unwrap();
    let host = host(state.path());
    let address_file = state.path().join("address_file");

    opener(state.path(), browser(state.path(), &address_file, 0))
        .open(PageScreen::Current)
        .unwrap();

    assert_eq!(std::fs::read_to_string(&address_file).unwrap(), host.url());
}

#[test]
fn each_tab_opens_the_page_of_its_own_reviewer_whichever_started_last() {
    let state = tempfile::tempdir().unwrap();
    let first = host_at(state.path(), &in_tab("t1"));
    let second = host_at(state.path(), &in_tab("t2"));
    let opened = |tab: &str| {
        let address_file = state.path().join(format!("opened-from-{tab}"));
        opener_at(
            state.path(),
            in_tab(tab),
            browser(state.path(), &address_file, 0),
        )
        .open(PageScreen::Current)
        .unwrap();
        std::fs::read_to_string(&address_file).unwrap()
    };

    assert_eq!(opened("t1"), first.url());
    assert_eq!(opened("t2"), second.url());
}

#[test]
fn the_opener_opens_the_page_on_its_past_rounds_at_the_loopback_address() {
    let state = tempfile::tempdir().unwrap();
    let host = host(state.path());
    let address_file = state.path().join("address_file");

    opener(state.path(), browser(state.path(), &address_file, 0))
        .open(PageScreen::PastRounds)
        .unwrap();

    assert_eq!(
        std::fs::read_to_string(&address_file).unwrap(),
        format!("{}#past-rounds", host.url())
    );
}

#[test]
fn without_an_open_reviewer_the_opener_opens_nothing_and_says_why() {
    let state = tempfile::tempdir().unwrap();
    let address_file = state.path().join("address_file");

    let failure = opener(state.path(), browser(state.path(), &address_file, 0))
        .open(PageScreen::Current)
        .unwrap_err();

    assert_eq!(failure.url, None);
    assert!(!address_file.exists());
}

#[test]
fn a_browser_that_fails_leaves_the_address_to_open_by_hand() {
    let state = tempfile::tempdir().unwrap();
    let host = host(state.path());
    let address_file = state.path().join("address_file");

    let failure = opener(state.path(), browser(state.path(), &address_file, 3))
        .open(PageScreen::Current)
        .unwrap_err();

    assert_eq!(failure.url.as_deref(), Some(host.url()));
    assert!(failure.reason.contains('3'), "{}", failure.reason);
}

#[test]
fn a_missing_browser_program_leaves_the_address_to_open_by_hand() {
    let state = tempfile::tempdir().unwrap();
    let host = host(state.path());
    let missing = state.path().join("no-such-browser");

    let failure = opener(
        state.path(),
        Browser::command(&missing.display().to_string()),
    )
    .open(PageScreen::Current)
    .unwrap_err();

    assert_eq!(failure.url.as_deref(), Some(host.url()));
}
