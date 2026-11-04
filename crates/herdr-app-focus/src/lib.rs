//! The pane the user focuses in herdr-app's window, which herdr-core does not know: herdr-app
//! keeps the focus of its screen in its home's `view.json`, by its own pane IDs, and its layout,
//! which tells the terminal each pane shows, in herdr-core's `herdr-app` document.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Deserialize;

/// The name of herdr-core's document that holds herdr-app's layout.
pub const LAYOUT_DOCUMENT: &str = "herdr-app";

/// The file of herdr-app's home where it writes what its screen shows.
const VIEW_FILE: &str = "view.json";

/// herdr-app's home: `~/.config/herdr-app`, or the folder `HERDR_APP_HOME` names when it starts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppHome(PathBuf);

/// What herdr-app's screen shows (`view.json`): the selected space, the selected tab of each
/// space, and the focused pane of each tab.
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

/// herdr-app's layout, from herdr-core's `herdr-app` document: the spaces, their tabs, and the
/// panes of each tab, nested in rows and columns.
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

/// A row, a column, or a block, which shows a terminal or a mod.
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

    /// herdr-app's home when nothing names another: `.config/herdr-app` in the user's `home`.
    pub fn default_in(home: &Path) -> Self {
        Self(home.join(".config").join("herdr-app"))
    }

    /// The herdr-app pane the user focuses: the focused pane of the selected tab of the selected
    /// space. `None` when herdr-app wrote no view, or one that names no focused pane: an unread
    /// view leaves the focus unknown, which the reviewer can do without.
    fn focused_pane(&self) -> Option<String> {
        let text = std::fs::read(self.0.join(VIEW_FILE)).ok()?;
        let view: View = serde_json::from_slice(&text).ok()?;
        view.focused_pane().map(str::to_owned)
    }

    /// The terminal the user focuses: the one the focused pane shows in `layout`; `None` when
    /// that pane shows a mod.
    pub fn focused_terminal(&self, layout: &Layout) -> Option<String> {
        layout.terminal_of(&self.focused_pane()?).map(str::to_owned)
    }

    /// Calls `changed` each time herdr-app writes its view, until the watch drops. herdr-app
    /// replaces `view.json` with a renamed file: the watch is on the home, not on the file.
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

/// Whether `event` may have changed the view: a file written or renamed to `view.json`, or
/// events lost. Opening or reading the view is no change: `changed` reads it, and would hear its
/// own read again without end.
fn rewrites_view(event: &notify::Event) -> bool {
    event.need_rescan()
        || !matches!(event.kind, EventKind::Access(_))
            && event
                .paths
                .iter()
                .any(|path| path.file_name().is_some_and(|name| name == VIEW_FILE))
}

/// A watch of herdr-app's view, which ends when it drops.
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
    /// The terminal the herdr-app pane `pane` shows; `None` for a mod, or a pane it lacks.
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
