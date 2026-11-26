use std::sync::{Arc, Mutex, PoisonError};

use component_core::EventEnvelope;
use crossbeam_channel::Sender as EventSender;
use review_explore_page_host::{
    NetworkAccess, NetworkListener, PageNetwork, TunnelProgram, TunnelReport,
};

pub(super) struct PageSharing {
    network: PageNetwork,
    events: EventSender<EventEnvelope>,
    applied: Mutex<Option<NetworkAccess>>,
    tunnel: TunnelProgram,
}

impl PageSharing {
    pub(super) fn new(
        network: PageNetwork,
        events: EventSender<EventEnvelope>,
        tunnel: TunnelProgram,
    ) -> Self {
        Self {
            network,
            events,
            applied: Mutex::new(None),
            tunnel,
        }
    }

    pub(super) fn open_tunnel(&self) {
        let events = self.events.clone();
        let report: TunnelReport = Arc::new(move |state| {
            let _ = events.send(EventEnvelope::new(ui_events::ExplorePageTunnel(state)));
        });
        self.network.open_tunnel(&self.tunnel, &report);
    }

    pub(super) fn close_tunnel(&self) {
        self.network.close_tunnel();
    }

    pub(super) fn apply(&self, access: &NetworkAccess) {
        let mut applied = self.applied.lock().unwrap_or_else(PoisonError::into_inner);
        if applied.as_ref() == Some(access) {
            return;
        }
        self.network.unshare();
        *applied = None;
        match NetworkListener::bind(access) {
            Ok(Some(listener)) => {
                let events = self.events.clone();
                self.network.share(listener, move |url| {
                    let shared = ui_events::ExplorePageShared(url.to_owned());
                    let _ = events.send(EventEnvelope::new(shared));
                });
                *applied = Some(access.clone());
            }
            Ok(None) => {
                self.send(ui_events::ExplorePageOffNetwork);
                *applied = Some(access.clone());
            }
            Err(error) => self.send(ui_events::ExplorePageNotShared(error.to_string())),
        }
    }

    fn send<E: component_core::ApplicationEvent>(&self, event: E) {
        let _ = self.events.send(EventEnvelope::new(event));
    }
}
