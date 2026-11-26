use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use herdr_client::protocol::{TabId, WorkspaceId};
use review_explore::{RoundOverview, TabTitle};
use review_explore_page::{
    CommandRefusal, CommandSender, PageCommand, PageRound, PastChange, PastRound, PastRoundEntry,
    PastRounds, PastRoundsPublisher, PublishedRound, Recovery, ReviewName, RoundPublisher,
    RoundStage,
};

use crate::{NetworkAccess, NetworkListener, PageDirectory, PageHost, PanePlace};

pub(crate) fn published(id: &str) -> PublishedRound<'_> {
    PublishedRound {
        id,
        review_unit: &REVIEW,
        design: None,
        changed_files: 0,
        author: review_explore::ChangeAuthor::Reviewer,
        cancellable: None,
        earlier: false,
        overview: &OVERVIEW,
        earlier_citations: &[],
    }
}

static REVIEW: std::sync::LazyLock<review_types::ReviewUnit> =
    std::sync::LazyLock::new(|| "review".into());

static OVERVIEW: RoundOverview = RoundOverview {
    rail: Vec::new(),
    decisions: Vec::new(),
    earlier: Vec::new(),
    title: TabTitle::AgentWorking,
    glossary: std::collections::BTreeMap::new(),
};

pub(crate) fn no_round() -> RoundStage {
    RoundStage::NoRound {
        start: "start".into(),
    }
}

pub(crate) fn working() -> RoundStage {
    RoundStage::AgentWorking {
        request: "turn".into(),
        sent_at_ms: None,
        finishing_since_ms: None,
        not_prepared: None,
        answer: None,
    }
}

pub(crate) fn loopback(first_port: u16) -> NetworkAccess {
    NetworkAccess {
        enabled: true,
        interface: Some(LOOPBACK.into()),
        first_port,
    }
}

#[cfg(target_os = "linux")]
pub(crate) const LOOPBACK: &str = "lo";
#[cfg(not(target_os = "linux"))]
pub(crate) const LOOPBACK: &str = "lo0";

pub(crate) fn path(url: &str) -> &str {
    let rest = url.split_once("://").unwrap().1;
    &rest[rest.find('/').unwrap()..]
}

pub(crate) fn status(
    address: SocketAddr,
    method: &str,
    target: &str,
    headers: &[(&str, &str)],
) -> u16 {
    let response = request(address, method, target, headers, "");
    response
        .split(' ')
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or_else(|| panic!("no status in {response:?}"))
}

pub(crate) fn request(
    address: SocketAddr,
    method: &str,
    target: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> String {
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut request = format!("{method} {target} HTTP/1.1\r\nConnection: close\r\n");
    for (name, value) in headers {
        let _ = write!(request, "{name}: {value}\r\n");
    }
    let _ = write!(request, "Content-Length: {}\r\n\r\n{body}", body.len());
    stream.write_all(request.as_bytes()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}

pub(crate) const PAST: &str = "past-1";

fn past_rounds() -> PastRounds {
    let publisher = PastRoundsPublisher::default();
    publisher.publish(vec![PastChange {
        title: "An earlier change".into(),
        references: change_references::ChangeReferences::default(),
        revision: review_repository::repository::ShortRevision::default(),
        rounds: vec![PastRoundEntry {
            id: PAST.into(),
            saved_at_ms: 1_000,
        }],
    }]);
    PastRounds::new(&publisher, |id| {
        (id == PAST).then(|| {
            PastRound::new(
                published(PAST),
                working(),
                ReviewName::default(),
                change_references::ChangeReferences::default(),
                None,
                1_000,
            )
        })
    })
}

pub(crate) fn page_host(state: &std::path::Path) -> (Arc<RoundPublisher>, PageHost) {
    let round = Arc::new(RoundPublisher::default());
    let owner = Arc::downgrade(&round);
    let commands = CommandSender::new(move |command, reply| match command {
        PageCommand::Recover(Recovery::Reset { .. }) => {
            if let Some(round) = owner.upgrade() {
                round.publish(None, no_round());
            }
            reply.send(Ok(()));
        }
        _ => reply.send(Err(CommandRefusal::Stale)),
    });
    let host = PageHost::start(
        PageRound::new(round.subscribe(), commands).with_past_rounds(past_rounds()),
        &PageDirectory::new(state),
        &PanePlace {
            workspace: WorkspaceId("w1".into()),
            tab: TabId("t1".into()),
        },
        std::path::Path::new("/repositories/drafts"),
    )
    .unwrap();
    (round, host)
}

pub(crate) struct Shared {
    pub(crate) round: Arc<RoundPublisher>,
    pub(crate) host: PageHost,
    pub(crate) address: SocketAddr,
    pub(crate) announced: mpsc::Receiver<String>,
    pub(crate) first: String,
    _state: tempfile::TempDir,
}

impl Shared {
    pub(crate) fn start() -> Self {
        let state = tempfile::tempdir().unwrap();
        let (round, host) = page_host(state.path());
        let (address, announced) = share_on_loopback(&host);
        let first = announced
            .recv_timeout(Duration::from_secs(5))
            .expect("the address of the page before any round");
        Self {
            round,
            host,
            address,
            announced,
            first,
            _state: state,
        }
    }

    pub(crate) fn replace_round(&self, id: &str) -> String {
        self.round.publish(Some(published(id)), working());
        self.next_announcement()
    }

    pub(crate) fn start_round(&self, id: &str) {
        self.round.publish(Some(published(id)), working());
        assert_eq!(
            self.next_announcement(),
            self.first,
            "the round keeps the address"
        );
    }

    pub(crate) fn next_announcement(&self) -> String {
        self.announced
            .recv_timeout(Duration::from_secs(5))
            .expect("an announcement")
    }

    pub(crate) fn status(&self, method: &str, target: &str, headers: &[(&str, &str)]) -> u16 {
        let host = self.address.to_string();
        let mut all = vec![("Host", host.as_str())];
        all.extend_from_slice(headers);
        status(self.address, method, target, &all)
    }

    pub(crate) fn open(&self, url: &str) -> u16 {
        self.status("GET", path(url), &[])
    }

    pub(crate) fn load(&self, url: &str) -> u16 {
        let token = url.rsplit('=').next().unwrap();
        let cookie = format!("explore_token_{}={token}", self.address.port());
        self.status("GET", "/", &[("Cookie", &cookie)])
    }
}

pub(crate) fn share_on_loopback(host: &PageHost) -> (SocketAddr, mpsc::Receiver<String>) {
    let listener = NetworkListener::bind(&loopback(0)).unwrap().unwrap();
    let address = listener.address();
    let (announce, announced) = mpsc::channel();
    host.network().share(listener, move |url| {
        let _ = announce.send(url.to_owned());
    });
    (address, announced)
}
