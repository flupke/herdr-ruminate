use std::sync::mpsc;
use std::thread;

use herdr_client::client::{EventCanceller, HerdrClient};
use herdr_client::protocol::{AgentStatus, HerdrEvent, PaneId};

pub struct HerdrEventWatch {
    events: mpsc::Receiver<HerdrEvent>,
    canceller: EventCanceller,
}

impl HerdrEventWatch {
    pub(crate) fn start(client: &HerdrClient) -> Self {
        let canceller = EventCanceller::default();
        let stream = client.subscribe_events(&canceller).unwrap();
        let (sender, events) = mpsc::channel();
        thread::spawn(move || stream.forward(|event| sender.send(event).is_ok()));
        Self { events, canceller }
    }

    pub fn wait_for(&self, what: &str, mut matches: impl FnMut(&HerdrEvent) -> bool) -> HerdrEvent {
        loop {
            let event = self
                .events
                .recv_timeout(crate::GUARD)
                .unwrap_or_else(|error| panic!("Herdr did not report {what}: {error}"));
            if matches(&event) {
                return event;
            }
        }
    }
}

impl Drop for HerdrEventWatch {
    fn drop(&mut self) {
        self.canceller.cancel();
    }
}

pub struct AgentStatusWatch {
    statuses: mpsc::Receiver<AgentStatus>,
    canceller: EventCanceller,
    pane: PaneId,
}

impl AgentStatusWatch {
    pub(crate) fn start(client: &HerdrClient, pane: &PaneId) -> Self {
        let canceller = EventCanceller::default();
        let stream = client.subscribe_agent_status(pane, &canceller).unwrap();
        let (sender, statuses) = mpsc::channel();
        thread::spawn(move || stream.forward(|status| sender.send(status).is_ok()));
        Self {
            statuses,
            canceller,
            pane: pane.clone(),
        }
    }

    pub fn wait_for(&self, status: AgentStatus) {
        self.wait_within(crate::GUARD, status);
    }

    pub fn wait_within(&self, guard: std::time::Duration, status: AgentStatus) {
        let mut seen = Vec::new();
        while seen.last() != Some(&status) {
            match self.statuses.recv_timeout(guard) {
                Ok(next) => seen.push(next),
                Err(error) => panic!(
                    "Herdr did not report the agent of {} {status:?}, only {seen:?}: {error}",
                    self.pane.0
                ),
            }
        }
    }
}

impl Drop for AgentStatusWatch {
    fn drop(&mut self) {
        self.canceller.cancel();
    }
}
