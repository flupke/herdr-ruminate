use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Deserialize;

// herdr-app keeps its layout in herdr-core's document of this name.
pub const LAYOUT_DOCUMENT: &str = "herdr-app";

// herdr-app writes what its screen shows to this file of its home.
const VIEW_FILE: &str = "view.json";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppHome(PathBuf);

#[derive(Debug, Default, Deserialize)]
struct View {
    #[serde(default)]
    active: Option<String>,
    #[serde(default)]
    spaces: HashMap<String, String>,
    #[serde(default)]
    tabs: HashMap<String, TabView>,
}

#[derive(Debug, Deserialize)]
struct TabView {
    #[serde(default)]
    focused: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Layout {
    #[serde(default)]
    spaces: Vec<SpaceLayout>,
}

#[derive(Debug, Deserialize)]
struct SpaceLayout {
    #[serde(default)]
    tabs: Vec<TabLayout>,
}

#[derive(Debug, Deserialize)]
struct TabLayout {
    root: Node,
}

#[derive(Debug, Deserialize)]
struct Node {
    id: String,
    #[serde(default)]
    content: Option<Content>,
    #[serde(default)]
    children: Vec<Node>,
}

#[derive(Debug, Deserialize)]
struct Content {
    #[serde(default)]
    terminal: Option<String>,
}

impl AppHome {
    pub fn new(path: PathBuf) -> Self {
        Self(path)
    }

    pub fn default_in(home: &Path) -> Self {
        Self(home.join(".config").join("herdr-app"))
    }

    fn focused_pane(&self) -> Option<String> {
        let text = std::fs::read(self.0.join(VIEW_FILE)).ok()?;
        let view: View = serde_json::from_slice(&text).ok()?;
        view.focused_pane().map(str::to_owned)
    }

    pub fn focused_terminal(&self, layout: &Layout) -> Option<String> {
        layout.terminal_of(&self.focused_pane()?).map(str::to_owned)
    }

    // herdr-app replaces `view.json` by renaming another file over it.
    pub fn watch_view(&self, changed: impl Fn() + Send + 'static) -> io::Result<ViewWatch> {
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                if event.as_ref().map_or(true, rewrites_view) {
                    changed();
                }
            })
            .map_err(io::Error::other)?;
        watcher
            .watch(&self.0, RecursiveMode::NonRecursive)
            .map_err(io::Error::other)?;
        Ok(ViewWatch { _watcher: watcher })
    }
}

// notify reports opening and reading a file as `Access` events, as the read after each change does.
fn rewrites_view(event: &notify::Event) -> bool {
    event.need_rescan()
        || !matches!(event.kind, EventKind::Access(_))
            && event
                .paths
                .iter()
                .any(|path| path.file_name().is_some_and(|name| name == VIEW_FILE))
}

#[derive(Debug)]
pub struct ViewWatch {
    _watcher: RecommendedWatcher,
}

impl View {
    fn focused_pane(&self) -> Option<&str> {
        let tab = self.spaces.get(self.active.as_ref()?)?;
        self.tabs.get(tab)?.focused.as_deref()
    }
}

impl Layout {
    fn terminal_of(&self, pane: &str) -> Option<&str> {
        self.spaces
            .iter()
            .flat_map(|space| &space.tabs)
            .find_map(|tab| tab.root.find(pane))
            .and_then(|node| node.content.as_ref()?.terminal.as_deref())
    }
}

impl Node {
    fn find(&self, id: &str) -> Option<&Self> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|child| child.find(id))
    }
}

#[cfg(test)]
#[path = "lib.tests.rs"]
mod tests;
