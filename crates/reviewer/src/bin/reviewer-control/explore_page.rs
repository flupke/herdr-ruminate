//! `reviewer-control explore-page`, the Herdr action that opens the Explore page of the open
//! reviewer of its tab in the default browser.
use std::env;
use std::path::PathBuf;

use herdr_client::protocol::PluginContext;
use review_explore_page_host::{Browser, PageDirectory, PageOpener, PanePlace};
use review_explore_page_opening::PageScreen;

/// The action, for the tab it runs in.
pub(super) struct OpenExplorePage(PageOpener);

impl OpenExplorePage {
    /// The action in the tab of Herdr's action context.
    pub(super) fn from_env() -> eyre::Result<Self> {
        let context: PluginContext = serde_json::from_str(
            &env::var("HERDR_PLUGIN_CONTEXT_JSON")
                .map_err(|_| eyre::eyre!("HERDR_PLUGIN_CONTEXT_JSON is not set"))?,
        )?;
        let state_dir = PathBuf::from(
            env::var_os("HERDR_PLUGIN_STATE_DIR")
                .ok_or_else(|| eyre::eyre!("HERDR_PLUGIN_STATE_DIR is not set"))?,
        );
        let place = PanePlace {
            workspace: context
                .workspace_id
                .ok_or_else(|| eyre::eyre!("Herdr gave the action no workspace"))?,
            tab: context
                .tab_id
                .ok_or_else(|| eyre::eyre!("Herdr gave the action no tab"))?,
        };
        Ok(Self(PageOpener::new(
            PageDirectory::new(&state_dir),
            place,
            Browser::from_env(),
        )))
    }

    pub(super) fn run(&self) -> eyre::Result<()> {
        Ok(self.0.open(PageScreen::Current)?)
    }
}
