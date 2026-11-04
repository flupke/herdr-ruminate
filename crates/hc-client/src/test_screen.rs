//! A screen of herdr-app for the tests, whose view they write as herdr-app does.

use herdr_app_focus::AppHome;
use serde_json::json;

/// A home of herdr-app whose screen shows the space `sp1` and its tab `tb1`.
pub(crate) struct Screen(tempfile::TempDir);

impl Screen {
    pub(crate) fn new() -> Self {
        Self(tempfile::tempdir().unwrap())
    }

    pub(crate) fn home(&self) -> AppHome {
        AppHome::new(self.0.path().to_owned())
    }

    /// Focuses the pane `pane` of the tab: a temporary file renamed over the view, as herdr-app
    /// writes it.
    pub(crate) fn focus(&self, pane: &str) {
        let written = self.0.path().join("view.json.tmp");
        let view =
            json!({"active": "sp1", "spaces": {"sp1": "tb1"}, "tabs": {"tb1": {"focused": pane}}});
        std::fs::write(&written, view.to_string()).unwrap();
        std::fs::rename(written, self.0.path().join("view.json")).unwrap();
    }
}
