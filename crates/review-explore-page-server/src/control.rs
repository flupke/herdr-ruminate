use axum::body::Bytes;
use axum::extract::{Path, Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use review_explore::{Question, StartBlock};
use review_explore_page::Token;

use crate::round_data;
use crate::sessions::{PageChange, Sessions, Step};

pub(crate) fn router(sessions: Sessions) -> Router {
    Router::new()
        .route("/test/sessions", post(open_session))
        .route("/test/sessions/{token}/answers", get(answers))
        .route("/test/sessions/{token}/diagram-errors", get(diagram_errors))
        .route("/test/sessions/{token}/starts", get(starts))
        .route(
            "/test/sessions/{token}/implementations",
            get(implementations),
        )
        .route("/test/sessions/{token}/quiz", get(quiz))
        .route(
            "/test/sessions/{token}/review-everything",
            post(|state, path| async move {
                block_starts(state, path, Some(StartBlock::NothingToReview))
            }),
        )
        .route("/test/sessions/{token}/mark-by-hand", post(mark_by_hand))
        .route("/test/sessions/{token}/actions", get(actions))
        .route("/test/sessions/{token}/messages", get(messages))
        .route("/test/sessions/{token}/agent-replies", post(agent_replies))
        .route(
            "/test/sessions/{token}/agent-replies-wide",
            post(agent_replies_wide),
        )
        .route(
            "/test/sessions/{token}/agent-replies-with-diagram",
            post(agent_replies_with_diagram),
        )
        .route(
            "/test/sessions/{token}/messages-not-delivered",
            post(messages_not_delivered),
        )
        .route(
            "/test/sessions/{token}/hold",
            post(|state, path| async move { change_page(state, path, PageChange::Hold) }),
        )
        .route(
            "/test/sessions/{token}/restart",
            post(|state, path| async move { change_page(state, path, PageChange::Restart) }),
        )
        .route(
            "/test/sessions/{token}/back",
            post(|state, path| async move { change_page(state, path, PageChange::Back) }),
        )
        .route("/test/sessions/{token}/{step}", post(step))
        .with_state(sessions)
}

#[derive(Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionRequest {
    data: Option<String>,
}

async fn open_session(State(sessions): State<Sessions>, body: Bytes) -> Response {
    let request = match optional_json::<SessionRequest>(&body) {
        Ok(request) => request.unwrap_or_default(),
        Err(error) => {
            return (StatusCode::BAD_REQUEST, format!("invalid session: {error}")).into_response();
        }
    };
    let data = match request.data.as_deref().map(round_data::by_name) {
        None => None,
        Some(Some(data)) => Some(data),
        Some(None) => return (StatusCode::BAD_REQUEST, "unknown data set").into_response(),
    };
    let token = Token::random();
    let body = serde_json::json!({ "token": token.to_string() });
    sessions.open(token, 0, data);
    Json(body).into_response()
}

fn optional_json<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<Option<T>, String> {
    if body.is_empty() {
        return Ok(None);
    }
    serde_json::from_slice(body)
        .map(Some)
        .map_err(|error| error.to_string())
}

async fn step(
    State(sessions): State<Sessions>,
    Path((token, step)): Path<(String, String)>,
    body: Bytes,
) -> Response {
    let Some(step) = Step::parse(&step) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let question = match optional_json::<Question>(&body) {
        Ok(question) => question,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                format!("invalid question: {error}"),
            )
                .into_response();
        }
    };
    if sessions.step(&token, step, question) {
        StatusCode::NO_CONTENT.into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

fn block_starts(
    State(sessions): State<Sessions>,
    Path(token): Path<String>,
    block: Option<StartBlock>,
) -> StatusCode {
    if sessions.block_starts(&token, block) {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

async fn mark_by_hand(State(sessions): State<Sessions>, Path(token): Path<String>) -> StatusCode {
    if sessions.mark_by_hand(&token) {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

fn change_page(
    State(sessions): State<Sessions>,
    Path(token): Path<String>,
    change: PageChange,
) -> StatusCode {
    if sessions.change_page(&token, change) {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

async fn answers(State(sessions): State<Sessions>, Path(token): Path<String>) -> Response {
    match sessions.answers(&token) {
        Some(answers) => Json(answers).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn diagram_errors(State(sessions): State<Sessions>, Path(token): Path<String>) -> Response {
    match sessions.diagram_errors(&token) {
        Some(errors) => Json(errors).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn starts(State(sessions): State<Sessions>, Path(token): Path<String>) -> Response {
    match sessions.starts(&token) {
        Some(starts) => Json(starts).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn implementations(State(sessions): State<Sessions>, Path(token): Path<String>) -> Response {
    match sessions.implementations(&token) {
        Some(implementations) => Json(implementations).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn quiz(State(sessions): State<Sessions>, Path(token): Path<String>) -> Response {
    match sessions.quiz(&token) {
        Some(quiz) => Json(quiz).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn found(changed: bool) -> StatusCode {
    if changed {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

async fn agent_replies(State(sessions): State<Sessions>, Path(token): Path<String>) -> StatusCode {
    found(sessions.agent_replies(&token))
}

async fn agent_replies_wide(
    State(sessions): State<Sessions>,
    Path(token): Path<String>,
) -> StatusCode {
    found(sessions.agent_replies_wide(&token))
}

async fn agent_replies_with_diagram(
    State(sessions): State<Sessions>,
    Path(token): Path<String>,
) -> StatusCode {
    found(sessions.agent_replies_with_diagram(&token))
}

async fn messages_not_delivered(
    State(sessions): State<Sessions>,
    Path(token): Path<String>,
) -> StatusCode {
    found(sessions.messages_not_delivered(&token))
}

async fn messages(State(sessions): State<Sessions>, Path(token): Path<String>) -> Response {
    match sessions.messages(&token) {
        Some(messages) => Json(messages).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn actions(State(sessions): State<Sessions>, Path(token): Path<String>) -> Response {
    match sessions.actions(&token) {
        Some(actions) => Json(actions).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub(crate) async fn away(
    State(sessions): State<Sessions>,
    request: Request,
    next: Next,
) -> Response {
    let restarting = request.uri().path() == "/ws"
        && request
            .headers()
            .get_all(header::COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .flat_map(|value| value.split(';'))
            .filter_map(|pair| pair.trim().split_once('='))
            .any(|(name, token)| name.starts_with("explore_token_") && sessions.away(token));
    if restarting {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    next.run(request).await
}
