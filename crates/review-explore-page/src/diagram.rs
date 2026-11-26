use std::sync::Arc;

use axum::http::{HeaderMap, HeaderValue, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{MethodRouter, get};

use crate::Rounds;
use crate::page::ExplorePage;

pub(crate) fn script_path() -> String {
    format!("/assets/{}", mermaid_js::FILE_NAME)
}

pub(crate) fn script_route<R: Rounds>() -> (String, MethodRouter<Arc<ExplorePage<R>>>) {
    (script_path(), get(script))
}

async fn script(headers: HeaderMap) -> Response {
    let cache = (
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
    let content_type = (
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/javascript"),
    );
    let vary = (header::VARY, HeaderValue::from_static("accept-encoding"));
    if accepts_gzip(&headers) {
        let encoding = (header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));
        ([content_type, cache, vary, encoding], mermaid_js::GZIPPED).into_response()
    } else {
        ([content_type, cache, vary], mermaid_js::script()).into_response()
    }
}

fn accepts_gzip(headers: &HeaderMap) -> bool {
    headers
        .get_all(header::ACCEPT_ENCODING)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .filter_map(|coding| coding.split(';').next())
        .any(|coding| coding.trim().eq_ignore_ascii_case("gzip"))
}
