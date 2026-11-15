//! `reviewer-control explore-page`, the Herdr action that opens the Explore page of the open
//! reviewer of its tab in the default browser, and the commands of herdr-app's page key, which
//! opens a review pane's page from another machine: `explore-page-address`, `explore-page-wait`
//! and `open-url`.
use std::env;
use std::io::Write as _;
use std::path::PathBuf;
use std::time::Duration;

use herdr_client::protocol::{PluginContext, TabId, WorkspaceId};
use review_explore_page_host::{Browser, PageDirectory, PageOpener, PanePlace, page_answers};
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

/// Runs `command`, one of herdr-app's page key's, with the arguments that follow its name.
pub(super) fn run_page_key_command(
    command: &str,
    args: impl Iterator<Item = String>,
) -> eyre::Result<()> {
    match command {
        "explore-page-address" => PrintPageAddress::from_args(args)?.run(),
        "explore-page-wait" => {
            WaitForPage::from_args(args)?.run();
            Ok(())
        }
        _ => open_url(args),
    }
}

/// `reviewer-control explore-page-address <space> <tab>`: prints the address of the Explore page
/// that the open reviewer of that herdr-app tab serves on this machine, for herdr-app's page key
/// (`herdr-app-mod/explore-page`), or exits with [`NO_PAGE`] when none serves one.
struct PrintPageAddress {
    place: PanePlace,
}

/// The exit status of `explore-page-address` when no reviewer of the tab serves a page.
const NO_PAGE: i32 = 3;

impl PrintPageAddress {
    /// The command for the arguments that follow its name.
    fn from_args(args: impl Iterator<Item = String>) -> eyre::Result<Self> {
        let [workspace, tab] = <[String; 2]>::try_from(args.collect::<Vec<_>>()).map_err(|_| {
            eyre::eyre!("usage: reviewer-control explore-page-address <space> <tab>")
        })?;
        Ok(Self {
            place: PanePlace {
                workspace: WorkspaceId(workspace),
                tab: TabId(tab),
            },
        })
    }

    fn run(&self) -> eyre::Result<()> {
        let directory = PageDirectory::new(&reviewer_dirs::state_root()?);
        let Some(url) = directory.address(&self.place)? else {
            std::process::exit(NO_PAGE);
        };
        println!("{url}");
        Ok(())
    }
}

/// `reviewer-control explore-page-wait <url>`: returns once the Explore page at `url`, on this
/// machine's loopback interface, stops answering, or once its standard output is closed. The page
/// key's ssh forward runs it as its remote command, so that the forward ends with the page, and
/// it with the forward: sshd closes the command's output once ssh goes, while the shell that runs
/// the command stays. It reads no record: the ssh command's environment may name another state
/// folder than the reviewer's.
struct WaitForPage {
    port: u16,
}

/// How often the page is asked whether it answers.
const PAGE_POLL: Duration = Duration::from_secs(2);

/// How many checks in a row the page must miss to count as closed: one slow answer is no close.
const MISSES_TO_CLOSE: u32 = 2;

impl WaitForPage {
    /// The command for the arguments that follow its name.
    fn from_args(args: impl Iterator<Item = String>) -> eyre::Result<Self> {
        let usage = || eyre::eyre!("usage: reviewer-control explore-page-wait <url>");
        let [url] = <[String; 1]>::try_from(args.collect::<Vec<_>>()).map_err(|_| usage())?;
        let port = url
            .strip_prefix("http://127.0.0.1:")
            .and_then(|rest| rest.split(['/', '?']).next())
            .and_then(|port| port.parse().ok())
            .ok_or_else(usage)?;
        Ok(Self { port })
    }

    fn run(&self) {
        let mut output = std::io::stdout();
        // A newline at each check, which ssh drops: writing it fails once sshd closed the output.
        let mut connected = || writeln!(output).and_then(|()| output.flush()).is_ok();
        let mut misses = 0;
        while misses < MISSES_TO_CLOSE && connected() {
            misses = if page_answers(self.port) {
                0
            } else {
                misses + 1
            };
            std::thread::sleep(PAGE_POLL);
        }
    }
}

/// `reviewer-control open-url <url>`: opens `url` in this machine's default browser, as the
/// review pane does (`BROWSER`, else `xdg-open` or `open`), for herdr-app's page key.
fn open_url(args: impl Iterator<Item = String>) -> eyre::Result<()> {
    let [url] = <[String; 1]>::try_from(args.collect::<Vec<_>>())
        .map_err(|_| eyre::eyre!("usage: reviewer-control open-url <url>"))?;
    Browser::from_env()
        .open(&url)
        .map_err(|error| eyre::eyre!(error))
}
