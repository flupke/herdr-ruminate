use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use tokio::sync::mpsc;

use crate::access::TokenCookie;
use crate::actions::Actions;
use crate::conversation::ThreadsFeed;
use crate::page::{ExplorePage, PageEvent};
use crate::past::{PastRound, PastRounds};
use crate::round::{PageRound, RoundFeed};
use crate::rpc::{
    Call, Notification, Outcome, Reply, Request, RoundCall, RpcError, Seq, StateParams,
};
use crate::status::StatusCard;
use crate::view::PageView;
use crate::{Rounds, notice::Notice};

pub fn page_listener(
    listener: tokio::net::TcpListener,
) -> impl axum::serve::Listener<Io = tokio::net::TcpStream, Addr = std::net::SocketAddr> {
    axum::serve::ListenerExt::tap_io(listener, |stream| {
        // Nagle's algorithm delays the socket's small messages.
        let _ = stream.set_nodelay(true);
    })
}

pub(crate) const HEARTBEAT: Duration = Duration::from_secs(10);

pub(crate) const TOKEN_ENDED: u16 = 4001;

const SEND_TIMEOUT: Duration = Duration::from_secs(10);

const GOING_AWAY: u16 = 1001;

#[derive(serde::Deserialize)]
pub(crate) struct SocketQuery {
    past: Option<String>,
}

pub(crate) async fn upgrade<R: Rounds>(
    State(page): State<Arc<ExplorePage<R>>>,
    Query(query): Query<SocketQuery>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    let Some(token) = TokenCookie::read(&headers).map(str::to_owned) else {
        return page.refuse(PageEvent::WrongToken).into_response();
    };
    let Some(round) = page.rounds().find(&token) else {
        return page.refuse(PageEvent::WrongToken).into_response();
    };
    upgrade.on_upgrade(move |socket| {
        Connection {
            page,
            token,
            round,
            asked_past: query.past,
            past: None,
            sent: None,
            pending: 0,
        }
        .run(socket)
    })
}

struct Connection<R> {
    page: Arc<ExplorePage<R>>,
    token: String,
    round: PageRound,
    asked_past: Option<String>,
    past: Option<PastRound>,
    sent: Option<Seq>,
    pending: usize,
}

enum Admission {
    Open,
    Waiting,
    Ended,
}

enum End {
    Closed,
    TokenEnded,
    OwnerGone,
}

impl<R: Rounds> Connection<R> {
    async fn run(mut self, mut socket: WebSocket) {
        let end = self.serve(&mut socket).await;
        let frame = match end {
            End::Closed => None,
            End::TokenEnded => Some(CloseFrame {
                code: TOKEN_ENDED,
                reason: "The token no longer opens a round".into(),
            }),
            End::OwnerGone => Some(CloseFrame {
                code: GOING_AWAY,
                reason: "The review tool stops".into(),
            }),
        };
        if let Some(frame) = frame {
            let _ = socket.send(Message::Close(Some(frame))).await;
        }
    }

    async fn serve(&mut self, socket: &mut WebSocket) -> End {
        let mut stages: RoundFeed = self.round.stages.clone();
        let mut picks = self.page.picks.subscribe();
        let mut threads = self
            .round
            .conversation
            .as_ref()
            .map(|conversation| conversation.threads.clone());
        let mut past_rounds = self.round.past.clone();
        let (done, mut replies) = mpsc::unbounded_channel::<Reply>();
        let mut heartbeat = tokio::time::interval(HEARTBEAT);
        heartbeat.tick().await;
        self.open_past().await;
        if self.send_view(socket).await.is_err() {
            return End::Closed;
        }
        loop {
            let sent = tokio::select! {
                incoming = socket.recv() => match incoming {
                    Some(Ok(Message::Text(text))) => {
                        match self.admission() {
                            Admission::Open => self.receive(text.as_str(), &done),
                            Admission::Waiting => self.refuse(text.as_str(), &done),
                            Admission::Ended => return End::TokenEnded,
                        }
                        Ok(())
                    }
                    Some(Ok(Message::Close(_)) | Err(_)) | None => return End::Closed,
                    Some(Ok(_)) => Ok(()),
                },
                changed = stages.changed() => {
                    if !changed {
                        return End::OwnerGone;
                    }
                    match self.admission() {
                        Admission::Ended => return End::TokenEnded,
                        Admission::Waiting => Ok(()),
                        Admission::Open => self.send_view(socket).await,
                    }
                }
                Ok(()) = picks.changed() => self.send_view(socket).await,
                true = threads_changed(&mut threads) => self.send_view(socket).await,
                true = past_rounds_changed(&mut past_rounds) => {
                    self.reload_past().await;
                    self.send_view(socket).await
                }
                Some(reply) = replies.recv() => {
                    self.pending -= 1;
                    if let Reply::Result { result: Outcome { reopen: Some(token), .. }, .. } = &reply {
                        self.token.clone_from(token);
                    }
                    let sent = match self.send_view(socket).await {
                        Ok(()) => send(socket, &reply).await,
                        Err(error) => Err(error),
                    };
                    if matches!(self.admission(), Admission::Ended) {
                        return End::TokenEnded;
                    }
                    sent
                }
                _ = heartbeat.tick() => match self.admission() {
                    Admission::Ended => return End::TokenEnded,
                    Admission::Waiting | Admission::Open => send(socket, &Notification::Ping).await,
                },
            };
            if sent.is_err() {
                return End::Closed;
            }
        }
    }

    fn admission(&self) -> Admission {
        if self.page.rounds().find(&self.token).is_some() {
            Admission::Open
        } else if self.pending > 0 {
            Admission::Waiting
        } else {
            Admission::Ended
        }
    }

    async fn open_past(&mut self) {
        let (Some(id), Some(past)) = (self.asked_past.take(), self.round.past.clone()) else {
            return;
        };
        if self.round.stages.round().as_deref() == Some(id.as_str()) {
            return;
        }
        self.past = past.open(id).await;
    }

    async fn reload_past(&mut self) {
        let (Some(shown), Some(past)) = (&self.past, &self.round.past) else {
            return;
        };
        if past.saved_at(shown.id()) == Some(shown.saved_at_ms) {
            return;
        }
        if let Some(round) = past.open(shown.id().to_owned()).await {
            self.past = Some(round);
        }
    }

    fn receive(&mut self, text: &str, done: &mpsc::UnboundedSender<Reply>) {
        let request = match serde_json::from_str::<Request>(text) {
            Ok(request) => request,
            Err(error) => {
                self.answer_error(text, RpcError::INVALID_REQUEST, error.to_string(), done);
                return;
            }
        };
        self.pending += 1;
        if self.past.is_some() && !matches!(request.call, Call::Round(RoundCall::Ping)) {
            let _ = done.send(Reply::Error {
                id: request.id,
                error: RpcError {
                    code: RpcError::STALE,
                    message: "A past round is read only".to_owned(),
                    data: Some(StatusCard::read_only()),
                },
            });
            return;
        }
        let page = self.page.clone();
        let round = self.round.clone();
        let done = done.clone();
        tokio::spawn(async move {
            let actions = Actions {
                page: &page,
                round: &round,
            };
            let result = actions.call(request.call).await;
            let _ = done.send(reply(request.id, result));
        });
    }

    fn refuse(&mut self, text: &str, done: &mpsc::UnboundedSender<Reply>) {
        let message = "The token no longer opens a round".to_owned();
        self.answer_error(text, RpcError::STALE, message, done);
    }

    fn answer_error(
        &mut self,
        text: &str,
        code: i32,
        message: String,
        done: &mpsc::UnboundedSender<Reply>,
    ) {
        let Some(id) = serde_json::from_str::<serde_json::Value>(text)
            .ok()
            .and_then(|value| value.get("id")?.as_u64())
        else {
            return;
        };
        self.pending += 1;
        let _ = done.send(Reply::Error {
            id,
            error: RpcError {
                code,
                message,
                data: None,
            },
        });
    }

    async fn send_view(&mut self, socket: &mut WebSocket) -> Result<(), axum::Error> {
        let snapshot = self.round.stages.latest();
        let threads = self
            .round
            .conversation
            .as_ref()
            .map(|conversation| conversation.threads.latest());
        let seq = Seq {
            revision: snapshot.revision,
            picks: self.page.picks.count(snapshot.round.as_deref()),
            threads: threads.as_ref().map_or(0, |threads| threads.revision),
            rounds: self.round.past.as_ref().map_or(0, PastRounds::revision),
        };
        if self.sent == Some(seq) {
            return Ok(());
        }
        let listed = self
            .round
            .past
            .as_ref()
            .map(|past| past.listed(snapshot.round.as_deref()));
        let view = match &self.past {
            Some(past) => PageView::of_past(past, &snapshot, threads.as_ref()),
            None => PageView::new(&snapshot, &self.page.picks, threads.as_ref()),
        }
        .listing(listed);
        let state = Notification::State(Box::new(StateParams {
            epoch: self.page.epoch().to_owned(),
            seq,
            view,
        }));
        send(socket, &state).await?;
        self.sent = Some(seq);
        Ok(())
    }
}

async fn threads_changed(threads: &mut Option<ThreadsFeed>) -> bool {
    let Some(feed) = threads else {
        return std::future::pending().await;
    };
    if feed.changed().await {
        return true;
    }
    *threads = None;
    false
}

async fn past_rounds_changed(past: &mut Option<PastRounds>) -> bool {
    let Some(rounds) = past else {
        return std::future::pending().await;
    };
    if rounds.changed().await {
        return true;
    }
    *past = None;
    false
}

fn reply(id: u64, result: Result<Outcome, Notice>) -> Reply {
    match result {
        Ok(result) => Reply::Result { id, result },
        Err(notice) => Reply::Error {
            id,
            error: RpcError {
                code: notice.code(),
                message: notice.message(),
                data: Some(StatusCard::of_notice(&notice)),
            },
        },
    }
}

async fn send(socket: &mut WebSocket, message: &impl serde::Serialize) -> Result<(), axum::Error> {
    let text = serde_json::to_string(message).map_err(axum::Error::new)?;
    tokio::time::timeout(SEND_TIMEOUT, socket.send(Message::Text(text.into())))
        .await
        .map_err(axum::Error::new)?
}

#[cfg(test)]
#[path = "socket.tests.rs"]
mod tests;
