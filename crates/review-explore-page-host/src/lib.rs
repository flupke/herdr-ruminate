//! The Explore page of an open reviewer: served on this machine for the round of the
//! reviewer's Explore session, and found by the Herdr action that opens it in a browser; and
//! served to the network for a phone, behind a new token for each round.
//!
//! The reviewer starts a [`PageHost`], which serves the page on a loopback port behind a token,
//! and leaves the page's address in the plugin's state directory, in the record of its
//! [place](PanePlace); a reviewer of the same review that starts again there keeps the address
//! and the token, so that an open page reconnects. The action reads that record through the
//! [`PageDirectory`]. Unless the [`NetworkAccess`] settings turn it off, the host also
//! [shares](PageNetwork::share) the page on a network interface, and announces the address of
//! each round's page, and of the start screen's while no round runs, for the pane's QR code; a
//! change of the settings moves it to another listener or [takes it off](PageNetwork::unshare)
//! the network. The reviewer can also [share the running round over a
//! tunnel](PageNetwork::open_tunnel), behind the same token on the network, until the round
//! ends. A [`PageOpener`] opens the page in the default browser: for the action, the page its
//! tab's record names; for the pane's Start, Start with Challenger, Open the Explore page and
//! Past rounds, the host's own ([`PageHost::opener`]).

mod browser;
mod network;

pub use browser::{Browser, PageOpener};
pub use network::{NetworkAccess, NetworkListener, PageNetwork, TunnelReport, TunnelState};
pub use quick_tunnel::TunnelProgram;

use std::fmt::Write as _;
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use herdr_client::protocol::{TabId, WorkspaceId};
use review_explore_page::{ExplorePage, Hosts, PageFiles, PageRound, Rounds, Token, page_listener};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::oneshot;

/// How long the action waits for a recorded page to answer.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(2);

/// Where open reviewers leave the address of their Explore page: one record per
/// [`PanePlace`], readable only by the user, since the address carries the page's token.
#[derive(Clone, Debug)]
pub struct PageDirectory(PathBuf);

/// The address of a page, as its record holds it, and the review it shows. The record stays
/// after its reviewer closes: the next reviewer of the same review serves the page at the same
/// address with the same token, so that a tab left open reconnects to it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct PageAddress {
    port: u16,
    /// The address that opens the page, with its token.
    url: String,
    /// The token of the address.
    #[serde(default)]
    token: String,
    /// The review the page shows: the root of its repository.
    #[serde(default)]
    review: PathBuf,
}

/// Where a review pane runs, which names the record of its page: its Herdr workspace, which is
/// herdr-app's space, and its tab. A tab holds one review pane, while a workspace holds one per
/// tab: the panes of a workspace's tabs each find their own page.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PanePlace {
    pub workspace: WorkspaceId,
    pub tab: TabId,
}

impl PageAddress {
    /// Whether the page answers: the reviewer that recorded it may have died without
    /// removing its record.
    fn answers(&self) -> bool {
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, self.port));
        let health = || -> io::Result<bool> {
            let mut stream = TcpStream::connect_timeout(&address, ANSWER_TIMEOUT)?;
            stream.set_read_timeout(Some(ANSWER_TIMEOUT))?;
            stream.set_write_timeout(Some(ANSWER_TIMEOUT))?;
            write!(
                stream,
                "GET /health HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
            )?;
            let mut status = [0; 12];
            stream.read_exact(&mut status)?;
            Ok(status.ends_with(b" 204"))
        };
        health().unwrap_or(false)
    }
}

impl PageDirectory {
    /// The records under the plugin's state directory.
    pub fn new(state_dir: &Path) -> Self {
        Self(state_dir.join("explore-page"))
    }

    /// The address of the page that the open reviewer at `place` serves, or `None` when no
    /// reviewer there serves one.
    pub fn address(&self, place: &PanePlace) -> io::Result<Option<String>> {
        let record = match fs::read(self.record(place)) {
            Ok(record) => record,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        Ok(serde_json::from_slice::<PageAddress>(&record)
            .ok()
            .filter(PageAddress::answers)
            .map(|address| address.url))
    }

    /// The record of the page at `place`, named after a digest of its IDs so that any IDs make
    /// a file name.
    fn record(&self, place: &PanePlace) -> PathBuf {
        // The NUL between the IDs, which neither holds, keeps two places from making one name.
        let digest = Sha256::new()
            .chain_update(place.workspace.0.as_bytes())
            .chain_update([0])
            .chain_update(place.tab.0.as_bytes())
            .finalize();
        let name = digest.iter().fold(String::new(), |mut name, byte| {
            let _ = write!(name, "{byte:02x}");
            name
        });
        self.0.join(name)
    }

    /// The address the reviewer of `review` at `place` that closed last served its page at,
    /// when no reviewer serves it now.
    fn closed(&self, place: &PanePlace, review: &Path) -> Option<PageAddress> {
        let record = fs::read(self.record(place)).ok()?;
        serde_json::from_slice::<PageAddress>(&record)
            .ok()
            .filter(|address| address.review == review && !address.token.is_empty())
            .filter(|address| !address.answers())
    }

    /// Writes `address` to `record`, one of this directory's records, replacing any earlier
    /// record whole.
    fn publish(&self, record: &Path, address: &PageAddress) -> io::Result<()> {
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.0)?;
        let partial = record.with_extension(format!("{}.partial", std::process::id()));
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&partial)?;
        file.write_all(&serde_json::to_vec(address)?)?;
        file.sync_all()?;
        fs::rename(&partial, record)
    }
}

/// The Explore page of one reviewer, served on this machine until the host drops. It shows the
/// round that the reviewer's Explore session publishes, and sends the session the reviewer's
/// commands.
pub struct PageHost {
    address: PageAddress,
    record: PathBuf,
    network: PageNetwork,
    stop: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl PageHost {
    /// Serves the page of `round`, of the review of the repository at `review`, on the loopback
    /// interface, and records its address for the review pane at `place` in `directory`. The
    /// page keeps the address and the token of the page of the same review that the reviewer at
    /// `place` served before it restarted, when its port is free; else it takes a free port, and
    /// a new token.
    pub fn start(
        round: PageRound,
        directory: &PageDirectory,
        place: &PanePlace,
        review: &Path,
    ) -> io::Result<Self> {
        let earlier = directory.closed(place, review);
        let (listener, token) = earlier
            .and_then(|earlier| {
                let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, earlier.port));
                Some((listener.ok()?, Token::chosen(earlier.token).ok()?))
            })
            .map_or_else(
                || {
                    std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
                        .map(|listener| (listener, Token::random()))
                },
                Ok,
            )?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let address = PageAddress {
            port,
            url: token.loopback_url(port),
            token: token.to_string(),
            review: review.to_owned(),
        };
        let page = ExplorePage::new(
            OneRound {
                token,
                round: round.clone(),
            },
            Hosts::loopback(port),
            PageFiles::embedded(),
            |_| {},
        );
        let app = page.into_router(axum::Router::new());
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let handle = runtime.handle().clone();
        let (stop, stopped) = oneshot::channel::<()>();
        let thread = thread::Builder::new()
            .name("explore-page".into())
            .spawn(move || {
                runtime.block_on(async move {
                    let Ok(listener) = tokio::net::TcpListener::from_std(listener) else {
                        return;
                    };
                    let _ = axum::serve(page_listener(listener), app)
                        .with_graceful_shutdown(async move {
                            let _ = stopped.await;
                        })
                        .await;
                });
            })?;
        let host = Self {
            address,
            record: directory.record(place),
            network: PageNetwork::new(round, handle),
            stop: Some(stop),
            thread: Some(thread),
        };
        directory.publish(&host.record, &host.address)?;
        Ok(host)
    }

    /// The address that opens the page, with its token.
    pub fn url(&self) -> &str {
        &self.address.url
    }

    /// Opens this page with `browser`, for the pane of the reviewer that serves it: never the
    /// page of another reviewer of the pane's tab, which may hold its record.
    pub fn opener(&self, browser: Browser) -> PageOpener {
        PageOpener::served(self.address.url.clone(), browser)
    }

    /// The page on the network, which the reviewer's settings share and take off it.
    pub fn network(&self) -> PageNetwork {
        self.network.clone()
    }
}

impl Drop for PageHost {
    /// Stops serving the page, and ends the tunnel that shares it. Its record stays, for the
    /// next reviewer of the review: the action finds no page there, since nothing answers at its
    /// address.
    fn drop(&mut self) {
        self.network.unshare();
        self.network.end_tunnels();
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The one round of a reviewer, behind the token of its page's address.
struct OneRound {
    token: Token,
    round: PageRound,
}

impl Rounds for OneRound {
    fn find(&self, token: &str) -> Option<PageRound> {
        self.token.matches(token).then(|| self.round.clone())
    }
}

#[cfg(test)]
mod test_page;
#[cfg(test)]
mod tests;
