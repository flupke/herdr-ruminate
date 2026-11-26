use herdr_app_focus::AppHome;
use serde_json::json;

pub(crate) struct Screen(tempfile::TempDir);

impl Screen {
    pub(crate) fn new() -> Self {
        Self(tempfile::tempdir().unwrap())
    }

    pub(crate) fn home(&self) -> AppHome {
        AppHome::new(self.0.path().to_owned())
    }

    // herdr-app writes its view as a temporary file that it renames over `view.json`.
    pub(crate) fn focus(&self, pane: &str) {
        let written = self.0.path().join("view.json.tmp");
        let view =
            json!({"active": "sp1", "spaces": {"sp1": "tb1"}, "tabs": {"tb1": {"focused": pane}}});
        std::fs::write(&written, view.to_string()).unwrap();
        std::fs::rename(written, self.0.path().join("view.json")).unwrap();
    }
}
