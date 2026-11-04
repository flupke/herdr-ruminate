use super::*;
use crate::test_screen::Screen;

/// A tab of two terminals and a mod; `extra` adds a fourth pane after them.
fn layout(extra: &str) -> Layout {
    serde_json::from_str(&format!(
        r#"{{"spaces": [{{"id": "sp1", "tabs": [{{"id": "tb1", "root": {{"id": "tb1", "type": "row", "children": [
            {{"id": "p1", "type": "block", "content": {{"terminal": "t1"}}}},
            {{"id": "p2", "type": "block", "content": {{"terminal": "t2"}}}},
            {{"id": "p3", "type": "block", "content": {{"mod": "digest"}}}}{extra}
        ]}}}}]}}]}}"#
    ))
    .unwrap()
}

fn terminal(address: &str) -> PaneId {
    PaneId(address.to_owned())
}

#[test]
fn each_terminal_the_user_focuses_is_told_once() {
    let screen = Screen::new();
    let mut focus = Focus::new(screen.home(), layout(""));
    screen.focus("p1");
    assert_eq!(focus.changed(), Some(terminal("t1")));
    // herdr-app writes its view for other reasons too, such as an agent's attention.
    assert_eq!(focus.changed(), None);
    screen.focus("p2");
    assert_eq!(focus.changed(), Some(terminal("t2")));
}

#[test]
fn a_terminal_focused_again_after_a_mod_is_told_again() {
    let screen = Screen::new();
    let mut focus = Focus::new(screen.home(), layout(""));
    screen.focus("p1");
    assert_eq!(focus.changed(), Some(terminal("t1")));
    screen.focus("p3");
    assert_eq!(focus.changed(), None);
    screen.focus("p1");
    assert_eq!(focus.changed(), Some(terminal("t1")));
}

#[test]
fn a_pane_focused_before_herdr_app_saved_its_layout_is_told_with_the_layout() {
    let screen = Screen::new();
    let mut focus = Focus::new(screen.home(), layout(""));
    screen.focus("p4");
    assert_eq!(focus.changed(), None);
    let saved = layout(r#", {"id": "p4", "type": "block", "content": {"terminal": "t4"}}"#);
    assert_eq!(focus.layout_changed(saved), Some(terminal("t4")));
}
