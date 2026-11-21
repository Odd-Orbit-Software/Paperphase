//! Read-only public transaction progress; no credentials, paths or activation actions.
use super::store::{Installation, public_job};
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use std::sync::Arc;
fn same_host_origin(headers: &HeaderMap, app_port: u16) -> bool {
    let Some(origin) = headers.get(header::ORIGIN) else {
        return true;
    };
    let Some(host) = headers.get(header::HOST).and_then(|h| h.to_str().ok()) else {
        return false;
    };
    let Ok(request) = url::Url::parse(&format!("http://{host}")) else {
        return false;
    };
    let Some(origin) = origin.to_str().ok().and_then(|s| url::Url::parse(s).ok()) else {
        return false;
    };
    origin.scheme() == "http"
        && origin.host_str() == request.host_str()
        && origin.port_or_known_default() == Some(app_port)
        && origin.username().is_empty()
        && origin.password().is_none()
}
async fn status(State(config): State<Arc<Installation>>, headers: HeaderMap) -> Response {
    let app_port = url::Url::parse(&config.health_url)
        .ok()
        .and_then(|u| u.port_or_known_default())
        .unwrap_or(8080);
    if !same_host_origin(&headers, app_port) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let mut response=match config.job(){
        Ok(job)=>Json(serde_json::json!({"job":job.map(public_job)})).into_response(),
        Err(_)=>(StatusCode::SERVICE_UNAVAILABLE,Json(serde_json::json!({"error":"Update status is unavailable. Check the supervisor log."}))).into_response(),
    };
    let result = response.headers_mut();
    result.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    result.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    if let Some(origin) = headers.get(header::ORIGIN) {
        result.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin.clone());
        result.insert(header::VARY, HeaderValue::from_static("Origin"));
    }
    response
}
pub fn router(config: Arc<Installation>) -> Router {
    Router::new()
        .route(
            "/help/updates",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
                    HELP_HTML,
                )
            }),
        )
        .route(
            "/help/updates-guide",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                    HELP_GUIDE,
                )
            }),
        )
        .route(
            "/app.css",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
                    include_bytes!("../../../../web/dist/app.css").as_slice(),
                )
            }),
        )
        .route("/api/update-status", get(status))
        .with_state(config)
}

pub const HELP_HTML: &[u8] = include_bytes!("../../../../web/dist/update-help.html");
pub const HELP_GUIDE: &[u8] = include_bytes!("../../../../docs/usage/release-updates.md");
