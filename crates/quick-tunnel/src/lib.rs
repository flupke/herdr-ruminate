mod address;
mod failure;
mod program;

pub use failure::TunnelFailure;
pub use program::TunnelProgram;

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

use agent_fork::{Exit, ForkCommand, ForkOutput, Launcher, RunningFork};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TunnelEvent {
    Opened { host: String },
    Failed(TunnelFailure),
}

pub struct QuickTunnel {
    fork: Arc<RunningFork>,
    stopped: Arc<AtomicBool>,
    // Linux sends cloudflared its parent-death signal when the thread that started it ends.
    _launcher: Launcher,
}

impl QuickTunnel {
    pub fn start(
        program: &TunnelProgram,
        target: SocketAddr,
        events: impl FnMut(TunnelEvent) + Send + 'static,
    ) -> Result<Self, TunnelFailure> {
        let executable = program.find()?;
        let launcher = Launcher::start(program.wrapper.clone());
        let (seen, sightings) = mpsc::channel();
        let fork = launcher
            .spawn(
                ForkCommand {
                    program: executable,
                    arguments: TunnelProgram::arguments(target),
                    directory: program.directory.clone(),
                    environment: program.environment.clone(),
                },
                String::new(),
                Box::new(Watch { seen, found: false }),
            )
            .map_err(|error| TunnelFailure::NotStarted(error.to_string()))?;
        let fork = Arc::new(fork);
        let stopped = Arc::new(AtomicBool::new(false));
        let supervisor = Supervisor {
            sightings,
            fork: Arc::clone(&fork),
            wait: program.address_wait,
            stopped: Arc::clone(&stopped),
        };
        thread::Builder::new()
            .name("quick tunnel".into())
            .spawn(move || supervisor.run(events))
            .map_err(|error| {
                fork.terminate();
                TunnelFailure::NotStarted(error.to_string())
            })?;
        Ok(Self {
            fork,
            stopped,
            _launcher: launcher,
        })
    }

    pub fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        self.fork.terminate();
    }
}

impl Drop for QuickTunnel {
    fn drop(&mut self) {
        self.stop();
    }
}

struct Supervisor {
    sightings: mpsc::Receiver<Sighting>,
    fork: Arc<RunningFork>,
    wait: Duration,
    stopped: Arc<AtomicBool>,
}

impl Supervisor {
    fn run(self, mut events: impl FnMut(TunnelEvent)) {
        let mut report = |event| {
            if !self.stopped.load(Ordering::SeqCst) {
                events(event);
            }
        };
        match self.address() {
            Ok(host) => {
                report(TunnelEvent::Opened { host });
                report(TunnelEvent::Failed(TunnelFailure::Ended(self.end())));
            }
            Err(failure) => report(TunnelEvent::Failed(failure)),
        }
    }

    fn address(&self) -> Result<String, TunnelFailure> {
        match self.sightings.recv_timeout(self.wait) {
            Ok(Sighting::Host(host)) => Ok(host),
            Ok(Sighting::End(exit)) => Err(TunnelFailure::Exited(exit)),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.fork.terminate();
                Err(TunnelFailure::NoAddress(self.wait))
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err(TunnelFailure::Exited(Exit::default()))
            }
        }
    }

    fn end(&self) -> Exit {
        match self.sightings.recv() {
            Ok(Sighting::End(exit)) => exit,
            Ok(Sighting::Host(_)) | Err(_) => Exit::default(),
        }
    }
}

enum Sighting {
    Host(String),
    End(Exit),
}

// cloudflared prints the address in its log, on the standard error.
struct Watch {
    seen: mpsc::Sender<Sighting>,
    found: bool,
}

impl Watch {
    fn read(&mut self, line: &str) {
        if self.found {
            return;
        }
        if let Some(host) = address::quick_tunnel_host(line) {
            self.found = true;
            let _ = self.seen.send(Sighting::Host(host.to_owned()));
        }
    }
}

impl ForkOutput for Watch {
    fn line(&mut self, line: &str) {
        self.read(line);
    }

    fn error_line(&mut self, line: &str) {
        self.read(line);
    }

    fn ended(self: Box<Self>, exit: Exit) {
        let _ = self.seen.send(Sighting::End(exit));
    }
}

#[cfg(test)]
mod tests;
