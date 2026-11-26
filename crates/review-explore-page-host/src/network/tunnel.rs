use std::net::{Ipv4Addr, TcpListener};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use quick_tunnel::{QuickTunnel, TunnelEvent, TunnelProgram};
use review_explore_page::{ExplorePage, Hosts, PageFiles, PageRound, Rounds, Token, page_listener};
pub use review_explore_page_tunnel::TunnelState;
use tokio::runtime::Handle;

use super::{PageNetwork, PageTask, RoundTokens, Shares};

pub type TunnelReport = Arc<dyn Fn(TunnelState) + Send + Sync>;

const NO_ROUND: &str = "no round runs, start one first";

#[derive(Default)]
pub(super) struct Tunnels {
    current: Option<TunnelShare>,
    opened: u64,
    stopping: Vec<thread::JoinHandle<()>>,
    watcher: Watcher,
}

#[derive(Clone, Default)]
struct Watcher(#[cfg(test)] Option<std::sync::mpsc::Sender<review_explore_page::RoundStage>>);

impl Watcher {
    #[cfg(test)]
    fn kept(&self, stages: &review_explore_page::RoundFeed) {
        if let Some(kept) = &self.0 {
            let _ = kept.send(stages.stage());
        }
    }

    #[cfg(not(test))]
    #[allow(clippy::unused_self)]
    fn kept(&self, _stages: &review_explore_page::RoundFeed) {}
}

impl Tunnels {
    pub(super) fn running(&self) -> bool {
        self.current.is_some()
    }

    fn current(&mut self, number: Option<u64>) -> Option<&mut TunnelShare> {
        self.current
            .as_mut()
            .filter(|tunnel| number.is_none_or(|number| tunnel.number == number))
    }

    fn stop_away(&mut self, tunnel: TunnelShare) {
        self.stopping.retain(|stop| !stop.is_finished());
        if let Ok(stop) = thread::Builder::new()
            .name("tunnel stop".into())
            .spawn(move || drop(tunnel))
        {
            self.stopping.push(stop);
        }
    }
}

impl Shares {
    fn end_tunnel(&mut self, number: Option<u64>, state: TunnelState) {
        if self.tunnels.current(number).is_none() {
            return;
        }
        let Some(tunnel) = self.tunnels.current.take() else {
            return;
        };
        self.release_tokens();
        tunnel.close();
        (tunnel.report)(state);
        self.tunnels.stop_away(tunnel);
    }
}

impl PageNetwork {
    pub fn open_tunnel(&self, program: &TunnelProgram, report: &TunnelReport) {
        let mut shares = self.lock();
        if let Some(tunnel) = shares.tunnels.current(None) {
            report(tunnel.state.clone());
            return;
        }
        match self.start_tunnel(&mut shares, program, report) {
            Ok(tunnel) => {
                report(TunnelState::Opening);
                shares.tunnels.current = Some(tunnel);
            }
            Err(reason) => {
                shares.release_tokens();
                report(TunnelState::Failed(reason));
            }
        }
    }

    pub fn close_tunnel(&self) {
        self.lock().end_tunnel(None, TunnelState::Off);
    }

    pub(crate) fn end_tunnels(&self) {
        let (tunnel, stopping) = {
            let mut shares = self.lock();
            let tunnel = shares.tunnels.current.take();
            shares.release_tokens();
            (tunnel, std::mem::take(&mut shares.tunnels.stopping))
        };
        drop(tunnel);
        for stop in stopping {
            let _ = stop.join();
        }
    }

    fn start_tunnel(
        &self,
        shares: &mut Shares,
        program: &TunnelProgram,
        report: &TunnelReport,
    ) -> Result<TunnelShare, String> {
        let round = self.round.stages().round().ok_or(NO_ROUND)?;
        let tokens = shares.tokens(&self.round, &self.runtime);
        let token = tokens.token_of(&round).ok_or(NO_ROUND)?;
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .and_then(|listener| {
                listener.set_nonblocking(true)?;
                Ok((listener.local_addr()?, listener))
            })
            .map_err(|error| format!("cannot listen for the tunnel: {error}"))?;
        let (local, listener) = listener;
        shares.tunnels.opened += 1;
        let number = shares.tunnels.opened;
        let network = self.downgrade();
        let process = QuickTunnel::start(program, local, move |event| {
            if let Some(network) = network.upgrade() {
                network.tunnel_event(number, event);
            }
        })
        .map_err(|failure| failure.to_string())?;
        Ok(TunnelShare {
            number,
            page: TunnelRounds {
                tokens,
                round: round.clone(),
                open: Arc::new(AtomicBool::new(true)),
            },
            token,
            state: TunnelState::Opening,
            report: Arc::clone(report),
            listener: Some(listener),
            process: Some(process),
            tasks: vec![self.watch_round(number, round, shares.tunnels.watcher.clone())],
        })
    }

    fn tunnel_event(&self, number: u64, event: TunnelEvent) {
        let mut shares = self.lock();
        match event {
            TunnelEvent::Opened { host } => {
                if let Some(tunnel) = shares.tunnels.current(Some(number)) {
                    tunnel.serve(&host, &self.runtime);
                }
            }
            TunnelEvent::Failed(failure) => {
                shares.end_tunnel(Some(number), TunnelState::Failed(failure.to_string()));
            }
        }
    }

    fn watch_round(&self, number: u64, round: String, watcher: Watcher) -> PageTask {
        let mut stages = self.round.stages().clone();
        let network = self.downgrade();
        PageTask::spawn(&self.runtime, async move {
            while stages.round().as_deref() == Some(round.as_str()) {
                watcher.kept(&stages);
                if !stages.changed().await {
                    break;
                }
            }
            if let Some(network) = network.upgrade() {
                thread::spawn(move || network.lock().end_tunnel(Some(number), TunnelState::Off));
            }
        })
    }
}

struct TunnelShare {
    number: u64,
    page: TunnelRounds,
    token: Token,
    state: TunnelState,
    report: TunnelReport,
    listener: Option<TcpListener>,
    process: Option<QuickTunnel>,
    tasks: Vec<PageTask>,
}

impl TunnelShare {
    fn serve(&mut self, host: &str, runtime: &Handle) {
        let Some(listener) = self.listener.take() else {
            return;
        };
        let page = ExplorePage::new(
            self.page.clone(),
            Hosts::tunnel(host),
            PageFiles::embedded(),
            |_| {},
        );
        let app = page.into_router(axum::Router::new());
        self.tasks.push(PageTask::spawn(runtime, async move {
            let Ok(listener) = tokio::net::TcpListener::from_std(listener) else {
                return;
            };
            let _ = axum::serve(page_listener(listener), app).await;
        }));
        self.state = TunnelState::Open {
            url: self.token.tunnel_url(host),
        };
        (self.report)(self.state.clone());
    }

    fn close(&self) {
        self.page.open.store(false, Ordering::SeqCst);
    }
}

impl Drop for TunnelShare {
    fn drop(&mut self) {
        self.close();
        for task in &self.tasks {
            task.abort();
        }
        for task in &self.tasks {
            task.wait_for_end();
        }
        drop(self.listener.take());
        drop(self.process.take());
    }
}

#[derive(Clone)]
struct TunnelRounds {
    tokens: RoundTokens,
    round: String,
    open: Arc<AtomicBool>,
}

impl Rounds for TunnelRounds {
    fn find(&self, token: &str) -> Option<PageRound> {
        let running = self.tokens.running();
        (self.open.load(Ordering::SeqCst) && running.as_deref() == Some(self.round.as_str()))
            .then(|| self.tokens.find(token))
            .flatten()
    }
}

#[cfg(test)]
#[path = "tunnel.tests.rs"]
mod tests;
