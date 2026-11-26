use std::sync::Arc;
use std::time::Duration;

use axum::extract::{DefaultBodyLimit, FromRequestParts, Path, Query, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use crate::access::{Hosts, TokenCookie};
use crate::blind::FirstPicks;
use crate::diagram;
use crate::files::PageFiles;
use crate::round::Rounds;

// Older browsers do not count the socket's `ws:` or `wss:` address as `'self'`. Mermaid writes
// inline styles into each diagram it draws.
fn content_security_policy(socket: &str) -> String {
    format!(
        "default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
         connect-src 'self' {socket}; img-src 'self' data:; worker-src 'self'; \
         manifest-src 'self'; form-action 'none'; base-uri 'none'; frame-ancestors 'none'; \
         report-uri /csp-report"
    )
}

const NO_STORE: (HeaderName, HeaderValue) =
    (header::CACHE_CONTROL, HeaderValue::from_static("no-store"));

const CSP_REPORT_LIMIT: usize = 16 * 1024;

const DEV_CHANGES_WAIT: Duration = Duration::from_secs(25);

#[derive(Debug)]
pub enum PageEvent {
    UnknownHost,
    ForeignOrigin,
    WrongToken,
    CspViolation(String),
}

pub struct ExplorePage<R> {
    rounds: R,
    hosts: Hosts,
    files: PageFiles,
    pub(crate) picks: FirstPicks,
    epoch: String,
    log: Box<dyn Fn(PageEvent) + Send + Sync>,
}

impl<R: Rounds> ExplorePage<R> {
    pub fn new(
        rounds: R,
        hosts: Hosts,
        files: PageFiles,
        log: impl Fn(PageEvent) + Send + Sync + 'static,
    ) -> Self {
        Self {
            rounds,
            hosts,
            files,
            picks: FirstPicks::default(),
            epoch: uuid::Uuid::new_v4().simple().to_string(),
            log: Box::new(log),
        }
    }

    pub fn into_router(self, routes: Router) -> Router {
        let page = Arc::new(self);
        let (script_path, script) = diagram::script_route::<R>();
        Router::new()
            .route("/", get(index::<R>))
            .route(&script_path, script)
            .route("/ws", get(crate::socket::upgrade::<R>))
            .route(
                "/token",
                get(|_: Admitted| async { StatusCode::NO_CONTENT }),
            )
            .route("/assets/{name}", get(asset::<R>))
            .route("/assets/client/{name}", get(client_asset::<R>))
            .route("/dev/changes", get(dev_changes::<R>))
            .route(
                "/csp-report",
                post(csp_report::<R>).layer(DefaultBodyLimit::max(CSP_REPORT_LIMIT)),
            )
            .route("/health", get(|| async { StatusCode::NO_CONTENT }))
            // Where a browser looks for the icon of a page that names none.
            .route(
                "/favicon.ico",
                get(|State(page): State<Arc<Self>>| async move {
                    serve_asset(&page.files, "favicon.ico")
                }),
            )
            .with_state(page.clone())
            .merge(routes)
            .layer(middleware::from_fn_with_state(page, admit_host::<R>))
    }

    pub(crate) fn rounds(&self) -> &R {
        &self.rounds
    }

    pub(crate) fn epoch(&self) -> &str {
        &self.epoch
    }

    fn admits(&self, headers: &HeaderMap) -> Result<(), Refused> {
        TokenCookie::read(headers)
            .and_then(|token| self.rounds.find(token))
            .map(|_| ())
            .ok_or_else(|| self.refuse(PageEvent::WrongToken))
    }

    fn open(&self, token: &str) -> Response {
        let Some(cookie) = self
            .rounds
            .find(token)
            .and_then(|_| TokenCookie::set(token))
        else {
            return self.refuse(PageEvent::WrongToken).into_response();
        };
        let mut response = Redirect::to("/").into_response();
        response.headers_mut().insert(header::SET_COOKIE, cookie);
        response
    }

    fn show(&self, headers: &HeaderMap) -> Response {
        if let Err(refused) = self.admits(headers) {
            return refused.into_response();
        }
        let host = headers
            .get(header::HOST)
            .and_then(|host| host.to_str().ok())
            .unwrap_or_default();
        let socket = self.hosts.socket(host);
        let Ok(policy) = HeaderValue::from_str(&content_security_policy(&socket)) else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        let policy = (header::CONTENT_SECURITY_POLICY, policy);
        // Browsers send the page's address to the sites its links open, unless a policy says not to.
        let referrer = (
            header::REFERRER_POLICY,
            HeaderValue::from_static("same-origin"),
        );
        let sniffing = (
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        );
        let html = self.files.shell();
        ([policy, NO_STORE, referrer, sniffing], Html(html)).into_response()
    }

    pub(crate) fn refuse(&self, event: PageEvent) -> Refused {
        self.log(event);
        Refused
    }

    fn log(&self, event: PageEvent) {
        (self.log)(event);
    }
}

pub(crate) struct Refused;

impl IntoResponse for Refused {
    fn into_response(self) -> Response {
        (
            StatusCode::FORBIDDEN,
            [NO_STORE],
            "This address does not open an Explore round any more. Open the page again from \
             the pane: its QR code, or the Herdr action that opens it.",
        )
            .into_response()
    }
}

async fn admit_host<R: Rounds>(
    State(page): State<Arc<ExplorePage<R>>>,
    mut request: Request,
    next: Next,
) -> Response {
    match page.hosts.admit(request.headers()) {
        Ok(cookies) => {
            cookies.receive(request.headers_mut());
            let mut response = next.run(request).await;
            cookies.send(response.headers_mut());
            response
        }
        Err(event) => page.refuse(event).into_response(),
    }
}

pub(crate) struct Admitted;

impl<R: Rounds> FromRequestParts<Arc<ExplorePage<R>>> for Admitted {
    type Rejection = Refused;

    async fn from_request_parts(
        parts: &mut Parts,
        page: &Arc<ExplorePage<R>>,
    ) -> Result<Self, Refused> {
        page.admits(&parts.headers).map(|()| Self)
    }
}

#[derive(Deserialize)]
struct PageQuery {
    token: Option<String>,
}

async fn index<R: Rounds>(
    State(page): State<Arc<ExplorePage<R>>>,
    Query(query): Query<PageQuery>,
    headers: HeaderMap,
) -> Response {
    match query.token {
        Some(token) => page.open(&token),
        None => page.show(&headers),
    }
}

async fn asset<R: Rounds>(
    State(page): State<Arc<ExplorePage<R>>>,
    Path(name): Path<String>,
) -> Response {
    serve_asset(&page.files, &name)
}

async fn client_asset<R: Rounds>(
    State(page): State<Arc<ExplorePage<R>>>,
    Path(name): Path<String>,
) -> Response {
    serve_asset(&page.files, &format!("client/{name}"))
}

fn serve_asset(files: &PageFiles, name: &str) -> Response {
    match files.asset(name) {
        Some((content_type, body)) => (
            [
                (header::CONTENT_TYPE, HeaderValue::from_static(content_type)),
                NO_STORE,
            ],
            body,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

#[derive(Deserialize)]
struct ChangesQuery {
    since: u64,
}

async fn dev_changes<R: Rounds>(
    State(page): State<Arc<ExplorePage<R>>>,
    _: Admitted,
    Query(query): Query<ChangesQuery>,
) -> Response {
    let Some(mut changes) = page.files.changes() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let _ = tokio::time::timeout(
        DEV_CHANGES_WAIT,
        changes.wait_for(|count| *count != query.since),
    )
    .await;
    let version = *changes.borrow();
    Json(serde_json::json!({ "version": version })).into_response()
}

async fn csp_report<R: Rounds>(
    State(page): State<Arc<ExplorePage<R>>>,
    body: String,
) -> StatusCode {
    page.log(PageEvent::CspViolation(body));
    StatusCode::NO_CONTENT
}

#[cfg(test)]
#[path = "page.tests.rs"]
mod tests;
