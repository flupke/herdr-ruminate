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

const ANSWER_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Debug)]
pub struct PageDirectory(PathBuf);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct PageAddress {
    port: u16,
    url: String,
    #[serde(default)]
    token: String,
    #[serde(default)]
    review: PathBuf,
}

// herdr-app runs a review pane in each tab of a space, which is a Herdr workspace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PanePlace {
    pub workspace: WorkspaceId,
    pub tab: TabId,
}

impl PageAddress {
    fn answers(&self) -> bool {
        page_answers(self.port)
    }
}

pub fn page_answers(port: u16) -> bool {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
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

impl PageDirectory {
    pub fn new(state_dir: &Path) -> Self {
        Self(state_dir.join("explore-page"))
    }

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

    fn record(&self, place: &PanePlace) -> PathBuf {
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

    fn closed(&self, place: &PanePlace, review: &Path) -> Option<PageAddress> {
        let record = fs::read(self.record(place)).ok()?;
        serde_json::from_slice::<PageAddress>(&record)
            .ok()
            .filter(|address| address.review == review && !address.token.is_empty())
            .filter(|address| !address.answers())
    }

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

pub struct PageHost {
    address: PageAddress,
    record: PathBuf,
    network: PageNetwork,
    stop: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl PageHost {
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

    pub fn url(&self) -> &str {
        &self.address.url
    }

    pub fn opener(&self, browser: Browser) -> PageOpener {
        PageOpener::served(self.address.url.clone(), browser)
    }

    pub fn network(&self) -> PageNetwork {
        self.network.clone()
    }
}

impl Drop for PageHost {
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
