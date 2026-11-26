use std::future::Future;
use std::io;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, UdpSocket};
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError, Weak, mpsc};
use std::task::{Context, Poll};
use std::time::Duration;

use review_explore_page::{ExplorePage, Hosts, PageFiles, PageRound, Rounds, Token, page_listener};
pub use review_explore_page_settings::NetworkAccess;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

mod tunnel;

use tunnel::Tunnels;
pub use tunnel::{TunnelReport, TunnelState};

const PORTS_TRIED: u16 = 10;

const STOP_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Clone)]
pub struct PageNetwork {
    round: PageRound,
    runtime: Handle,
    shares: Arc<Mutex<Shares>>,
}

#[derive(Clone)]
struct WeakNetwork {
    round: PageRound,
    runtime: Handle,
    shares: Weak<Mutex<Shares>>,
}

impl WeakNetwork {
    fn upgrade(&self) -> Option<PageNetwork> {
        Some(PageNetwork {
            round: self.round.clone(),
            runtime: self.runtime.clone(),
            shares: self.shares.upgrade()?,
        })
    }
}

#[derive(Default)]
struct Shares {
    tokens: Option<TokenRenewal>,
    listener: Option<NetworkShare>,
    tunnels: Tunnels,
}

impl Shares {
    fn tokens(&mut self, round: &PageRound, runtime: &Handle) -> RoundTokens {
        self.tokens
            .get_or_insert_with(|| TokenRenewal::start(round, runtime))
            .tokens
            .clone()
    }

    fn release_tokens(&mut self) {
        if self.listener.is_none() && !self.tunnels.running() {
            self.tokens = None;
        }
    }
}

impl PageNetwork {
    pub(crate) fn new(round: PageRound, runtime: Handle) -> Self {
        Self {
            round: round.without_past_rounds(),
            runtime,
            shares: Arc::default(),
        }
    }

    pub fn share(
        &self,
        listener: NetworkListener,
        announce: impl Fn(&str) + Send + Sync + 'static,
    ) {
        self.unshare();
        let address = listener.address();
        let mut shares = self.lock();
        let tokens = shares.tokens(&self.round, &self.runtime);
        let page = ExplorePage::new(
            tokens.clone(),
            Hosts::network(address),
            PageFiles::embedded(),
            |_| {},
        );
        let app = page.into_router(axum::Router::new());
        let listener = listener.into_std();
        let server = PageTask::spawn(&self.runtime, async move {
            let Ok(listener) = tokio::net::TcpListener::from_std(listener) else {
                return;
            };
            let _ = axum::serve(page_listener(listener), app).await;
        });
        tokens.announce_to(Some(Announcer {
            address,
            announce: Box::new(announce),
        }));
        shares.listener = Some(NetworkShare { tokens, server });
    }

    pub fn unshare(&self) {
        let mut shares = self.lock();
        drop(shares.listener.take());
        shares.release_tokens();
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Shares> {
        self.shares.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn downgrade(&self) -> WeakNetwork {
        WeakNetwork {
            round: self.round.clone(),
            runtime: self.runtime.clone(),
            shares: Arc::downgrade(&self.shares),
        }
    }
}

struct TokenRenewal {
    tokens: RoundTokens,
    task: JoinHandle<()>,
}

impl TokenRenewal {
    fn start(round: &PageRound, runtime: &Handle) -> Self {
        let tokens = RoundTokens::new(round.clone());
        let mut stages = round.stages().clone();
        let renewing = tokens.clone();
        let task = runtime.spawn(async move {
            loop {
                renewing.renew();
                if !stages.changed().await {
                    return;
                }
            }
        });
        Self { tokens, task }
    }
}

impl Drop for TokenRenewal {
    fn drop(&mut self) {
        self.tokens.close();
        self.task.abort();
    }
}

struct NetworkShare {
    tokens: RoundTokens,
    server: PageTask,
}

impl Drop for NetworkShare {
    fn drop(&mut self) {
        self.tokens.announce_to(None);
        self.server.abort();
        self.server.wait_for_end();
    }
}

struct PageTask {
    handle: JoinHandle<()>,
    ended: mpsc::Receiver<()>,
}

impl PageTask {
    fn spawn(runtime: &Handle, future: impl Future<Output = ()> + Send + 'static) -> Self {
        let (sender, ended) = mpsc::channel();
        let handle = runtime.spawn(Signalled {
            future: Box::pin(future),
            _ended: sender,
        });
        Self { handle, ended }
    }

    fn abort(&self) {
        self.handle.abort();
    }

    fn wait_for_end(&self) {
        let _ = self.ended.recv_timeout(STOP_TIMEOUT);
    }
}

struct Signalled {
    future: Pin<Box<dyn Future<Output = ()> + Send>>,
    _ended: mpsc::Sender<()>,
}

impl Future for Signalled {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<()> {
        self.future.as_mut().poll(context)
    }
}

pub struct NetworkListener {
    listener: TcpListener,
    address: SocketAddr,
}

impl NetworkListener {
    pub fn bind(access: &NetworkAccess) -> io::Result<Option<Self>> {
        if !access.enabled {
            return Ok(None);
        }
        let ip = match &access.interface {
            Some(name) => interface_address(name)?,
            None => default_address()?,
        };
        let first = access.first_port;
        let ports = if first == 0 {
            0..=0
        } else {
            first..=first.saturating_add(PORTS_TRIED - 1)
        };
        let mut last = None;
        for port in ports {
            match TcpListener::bind((ip, port)) {
                Ok(listener) => {
                    listener.set_nonblocking(true)?;
                    let address = listener.local_addr()?;
                    return Ok(Some(Self { listener, address }));
                }
                Err(error) if error.kind() == io::ErrorKind::AddrInUse => last = Some(error),
                Err(error) => return Err(error),
            }
        }
        Err(last.unwrap_or_else(|| io::Error::other("no port to try")))
    }

    pub(crate) fn address(&self) -> SocketAddr {
        self.address
    }

    fn into_std(self) -> TcpListener {
        self.listener
    }
}

fn interface_address(name: &str) -> io::Result<Ipv4Addr> {
    nix::ifaddrs::getifaddrs()?
        .filter(|interface| interface.interface_name == name)
        .find_map(|interface| Some(interface.address?.as_sockaddr_in()?.ip()))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("the interface {name} has no IPv4 address"),
            )
        })
}

// Connecting a UDP socket sends nothing: it only picks the interface.
fn default_address() -> io::Result<Ipv4Addr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
    socket.connect((Ipv4Addr::new(192, 0, 2, 1), 9))?;
    match socket.local_addr()?.ip() {
        std::net::IpAddr::V4(ip) if !ip.is_loopback() && !ip.is_unspecified() => {
            on_local_network(ip, interface_of(ip).as_deref())
        }
        _ => Err(io::Error::new(
            io::ErrorKind::NotFound,
            "this machine has no network address",
        )),
    }
}

fn interface_of(ip: Ipv4Addr) -> Option<String> {
    nix::ifaddrs::getifaddrs()
        .ok()?
        .find(|interface| {
            interface.address.and_then(|address| {
                address
                    .as_sockaddr_in()
                    .map(nix::sys::socket::SockaddrIn::ip)
            }) == Some(ip)
        })
        .map(|interface| interface.interface_name)
}

fn on_local_network(ip: Ipv4Addr, interface: Option<&str>) -> io::Result<Ipv4Addr> {
    let [first, second, ..] = ip.octets();
    let shared = first == 100 && second & 0b1100_0000 == 64;
    if ip.is_private() || ip.is_link_local() || shared {
        return Ok(ip);
    }
    let name = interface.unwrap_or("its interface");
    Err(io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!(
            "{ip} is a public address; name {name} in the Interface setting to serve the page to \
             the internet"
        ),
    ))
}

#[derive(Clone)]
struct RoundTokens {
    round: PageRound,
    current: Arc<Mutex<Current>>,
}

#[derive(Default)]
struct Current {
    token: Option<RoundToken>,
    closed: bool,
    announcer: Option<Announcer>,
}

struct Announcer {
    address: SocketAddr,
    announce: Box<dyn Fn(&str) + Send + Sync>,
}

impl Announcer {
    fn announce(&self, token: &Token) {
        (self.announce)(&token.url(self.address));
    }
}

struct RoundToken {
    round: Option<String>,
    token: Token,
}

impl RoundToken {
    fn opens(&self, running: Option<&String>) -> bool {
        self.round.is_none() || self.round.as_ref() == running
    }
}

impl RoundTokens {
    fn new(round: PageRound) -> Self {
        Self {
            round,
            current: Arc::default(),
        }
    }

    fn renew(&self) {
        let mut current = self.lock();
        self.renew_locked(&mut current);
    }

    fn running(&self) -> Option<String> {
        self.round.stages().round()
    }

    fn token_of(&self, round: &str) -> Option<Token> {
        let mut current = self.lock();
        self.renew_locked(&mut current);
        current
            .token
            .as_ref()
            .filter(|token| token.round.as_deref() == Some(round))
            .map(|token| token.token.clone())
    }

    fn announce_to(&self, announcer: Option<Announcer>) {
        let mut current = self.lock();
        if let (Some(announcer), Some(token)) = (&announcer, &current.token) {
            announcer.announce(&token.token);
        }
        current.announcer = announcer;
    }

    fn close(&self) {
        let mut current = self.lock();
        current.closed = true;
        current.token = None;
        current.announcer = None;
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Current> {
        self.current.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn renew_locked(&self, current: &mut Current) {
        if current.closed {
            return;
        }
        let running = self.round.stages().round();
        let token = &mut current.token;
        match token.as_mut() {
            Some(token) if token.round == running => return,
            Some(token) if token.round.is_none() => token.round = running,
            _ => {
                *token = Some(RoundToken {
                    round: running,
                    token: Token::random(),
                });
            }
        }
        if let (Some(token), Some(announcer)) = (token, &current.announcer) {
            announcer.announce(&token.token);
        }
    }
}

impl Rounds for RoundTokens {
    fn find(&self, token: &str) -> Option<PageRound> {
        let running = self.round.stages().round();
        let current = self.lock();
        current
            .token
            .as_ref()
            .filter(|current| current.opens(running.as_ref()) && current.token.matches(token))
            .map(|_| self.round.clone())
    }

    fn after_reset(&self) -> Option<Token> {
        let mut current = self.lock();
        self.renew_locked(&mut current);
        current
            .token
            .as_ref()
            .filter(|current| current.round.is_none())
            .map(|current| current.token.clone())
    }
}

#[cfg(test)]
#[path = "network.tests.rs"]
mod tests;
