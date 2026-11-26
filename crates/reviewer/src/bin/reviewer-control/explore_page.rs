use std::env;
use std::io::Write as _;
use std::path::PathBuf;
use std::time::Duration;

use herdr_client::protocol::{PluginContext, TabId, WorkspaceId};
use review_explore_page_host::{Browser, PageDirectory, PageOpener, PanePlace, page_answers};
use review_explore_page_opening::PageScreen;

pub(super) struct OpenExplorePage(PageOpener);

impl OpenExplorePage {
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

struct PrintPageAddress {
    place: PanePlace,
}

const NO_PAGE: i32 = 3;

impl PrintPageAddress {
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

// sshd closes an ssh command's output once ssh goes, while the shell that runs it stays.
// An ssh command's environment may name another state folder than the reviewer's.
struct WaitForPage {
    port: u16,
}

const PAGE_POLL: Duration = Duration::from_secs(2);

const MISSES_TO_CLOSE: u32 = 2;

impl WaitForPage {
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
        // ssh drops a lone newline: writing it fails once sshd closed the output.
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

fn open_url(args: impl Iterator<Item = String>) -> eyre::Result<()> {
    let [url] = <[String; 1]>::try_from(args.collect::<Vec<_>>())
        .map_err(|_| eyre::eyre!("usage: reviewer-control open-url <url>"))?;
    Browser::from_env()
        .open(&url)
        .map_err(|error| eyre::eyre!(error))
}
