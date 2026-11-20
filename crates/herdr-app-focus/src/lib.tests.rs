use std::sync::mpsc;

use super::*;

/// A layout of two spaces as herdr-core's `herdr-app` document holds it (herdr-app 0.1.13): a
/// tab of a terminal beside a column of a terminal over a mod, and a tab of one terminal.
fn layout() -> Layout {
    serde_json::from_str(
        r#"{"spaces": [
            {"id": "sp1", "folder": "/src/a", "name": "", "tabs": [
                {"id": "tb1", "name": "", "root": {"id": "tb1", "type": "row", "children": [
                    {"id": "p1", "type": "block", "content": {"terminal": "t1"}, "size": {"weight": 0.5}},
                    {"id": "c1", "type": "column", "children": [
                        {"id": "p2", "type": "block", "content": {"terminal": "t2"}},
                        {"id": "p3", "type": "block", "content": {"mod": "digest"}}
                    ]}
                ]}}
            ]},
            {"id": "sp2", "folder": "/src/b", "name": "b", "tabs": [
                {"id": "tb2", "name": "", "root": {"id": "tb2", "type": "row", "children": [
                    {"id": "p4", "type": "block", "content": {"terminal": "t4"}}
                ]}}
            ]}
        ]}"#,
    )
    .unwrap()
}

#[test]
fn a_pane_shows_the_terminal_of_its_block_however_deep() {
    let layout = layout();
    assert_eq!(layout.terminal_of("p1"), Some("t1"));
    assert_eq!(layout.terminal_of("p2"), Some("t2"));
    assert_eq!(layout.terminal_of("p4"), Some("t4"));
}

#[test]
fn a_mod_a_split_and_an_unknown_pane_show_no_terminal() {
    let layout = layout();
    assert_eq!(layout.terminal_of("p3"), None);
    assert_eq!(layout.terminal_of("c1"), None);
    assert_eq!(layout.terminal_of("p9"), None);
}

/// A home whose `view.json` is `view`.
fn home_with_view(view: &str) -> (tempfile::TempDir, AppHome) {
    let folder = tempfile::tempdir().unwrap();
    std::fs::write(folder.path().join("view.json"), view).unwrap();
    let home = AppHome::new(folder.path().to_owned());
    (folder, home)
}

#[test]
fn the_focus_is_the_focused_pane_of_the_selected_tab_of_the_selected_space() {
    let (_folder, home) = home_with_view(
        r#"{
            "active": "sp2",
            "spaces": {"sp1": "tb1", "sp2": "tb2"},
            "tabs": {
                "tb1": {"focused": "p2", "before": ["p1"], "zoomed": null, "settled": null},
                "tb2": {"focused": "p4", "before": [], "zoomed": null, "settled": null},
                "tb3": {"focused": "p5", "before": [], "zoomed": null, "settled": null}
            },
            "recent": [],
            "attention": {}
        }"#,
    );
    assert_eq!(home.focused_pane().as_deref(), Some("p4"));
    assert_eq!(home.focused_terminal(&layout()).as_deref(), Some("t4"));
}

#[test_case::test_case(r#"{"active": null, "spaces": {}, "tabs": {}}"#; "no space")]
#[test_case::test_case(r#"{"active": "sp1", "spaces": {"sp1": "tb1"}, "tabs": {}}"#; "a tab never focused")]
#[test_case::test_case(r#"{"active": "sp1", "spaces": {"sp1": "tb1"}, "tabs": {"tb1": {"focused": "p3"}}}"#; "a mod")]
#[test_case::test_case("{"; "a torn view")]
fn a_view_with_no_focused_terminal_focuses_none(view: &str) {
    let (_folder, home) = home_with_view(view);
    assert_eq!(home.focused_terminal(&layout()), None);
}

#[test]
fn a_home_without_a_view_focuses_nothing() {
    let folder = tempfile::tempdir().unwrap();
    assert_eq!(AppHome::new(folder.path().to_owned()).focused_pane(), None);
}

#[test]
fn the_watch_hears_herdr_app_replace_its_view() {
    let folder = tempfile::tempdir().unwrap();
    let home = AppHome::new(folder.path().to_owned());
    let (changed, heard) = mpsc::channel();
    let _watch = home
        .watch_view(move || {
            let _ = changed.send(());
        })
        .unwrap();
    // As herdr-app writes it: a temporary file, renamed over the view.
    let written = folder.path().join("view.json.tmp.1.1");
    std::fs::write(&written, "{}").unwrap();
    std::fs::rename(&written, folder.path().join("view.json")).unwrap();
    heard.recv().unwrap();
}

fn event(kind: EventKind, file: &str) -> notify::Event {
    notify::Event::new(kind).add_path(PathBuf::from("/home/u/.config/herdr-app").join(file))
}

#[test]
fn a_view_written_or_renamed_over_is_a_change() {
    use notify::event::{CreateKind, DataChange, ModifyKind, RenameMode};
    assert!(rewrites_view(&event(
        EventKind::Modify(ModifyKind::Name(RenameMode::To)),
        "view.json"
    )));
    assert!(rewrites_view(&event(
        EventKind::Create(CreateKind::File),
        "view.json"
    )));
    assert!(rewrites_view(&event(
        EventKind::Modify(ModifyKind::Data(DataChange::Any)),
        "view.json"
    )));
    assert!(rewrites_view(
        &notify::Event::new(EventKind::Other).set_flag(notify::event::Flag::Rescan)
    ));
}

#[test]
fn reading_the_view_or_writing_another_file_is_no_change() {
    use notify::event::{AccessKind, AccessMode, ModifyKind, RenameMode};
    // The read of the view that follows each change.
    assert!(!rewrites_view(&event(
        EventKind::Access(AccessKind::Open(AccessMode::Any)),
        "view.json"
    )));
    assert!(!rewrites_view(&event(
        EventKind::Access(AccessKind::Close(AccessMode::Read)),
        "view.json"
    )));
    assert!(!rewrites_view(&event(
        EventKind::Modify(ModifyKind::Name(RenameMode::To)),
        "shared.json"
    )));
}
