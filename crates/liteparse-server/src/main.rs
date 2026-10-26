//! Minimal HTTP API around the `liteparse` library.
//!
//! Routes:
//! - `GET  /health`  liveness probe
//! - `POST /parse`   multipart upload (`file` field) or raw body; options as query params

use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, FromRequest, Multipart, Query, Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use liteparse::{LiteParse, LiteParseConfig, OutputFormat, output::{json, text}, types::PdfInput};
use serde::Deserialize;
use serde_json::json;
use std::{env, sync::Arc};
use tokio::sync::Semaphore;

#[derive(Clone)]
struct AppState {
    /// Bounds concurrent parses; each parse is CPU and memory heavy.
    permits: Arc<Semaphore>,
    api_key: Option<Arc<str>>,
}

#[derive(Deserialize, Default)]
struct ParseParams {
    /// json (default), text, or markdown
    format: Option<String>,
    ocr: Option<bool>,
    ocr_language: Option<String>,
    ocr_server_url: Option<String>,
    target_pages: Option<String>,
    max_pages: Option<usize>,
    dpi: Option<f32>,
    password: Option<String>,
}

struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

fn bad_request(msg: impl ToString) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, msg.to_string())
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
}

async fn require_key(State(state): State<AppState>, req: Request, next: Next) -> Response {
    if let Some(key) = &state.api_key {
        let bearer = req
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));
        let header_key = req.headers().get("x-api-key").and_then(|v| v.to_str().ok());
        if bearer != Some(&**key) && header_key != Some(&**key) {
            return ApiError(StatusCode::UNAUTHORIZED, "invalid or missing API key".into())
                .into_response();
        }
    }
    next.run(req).await
}

async fn parse(
    State(state): State<AppState>,
    Query(p): Query<ParseParams>,
    req: Request,
) -> Result<Response, ApiError> {
    let is_multipart = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("multipart/form-data"));

    let data: Vec<u8> = if is_multipart {
        let mut mp = Multipart::from_request(req, &()).await.map_err(bad_request)?;
        let mut found = None;
        while let Some(field) = mp.next_field().await.map_err(bad_request)? {
            if field.name() == Some("file") {
                found = Some(field.bytes().await.map_err(bad_request)?.to_vec());
                break;
            }
        }
        found.ok_or_else(|| bad_request("multipart body needs a `file` field"))?
    } else {
        Bytes::from_request(req, &()).await.map_err(bad_request)?.to_vec()
    };
    if data.is_empty() {
        return Err(bad_request("empty document"));
    }

    let format = match p.format.as_deref().unwrap_or("json") {
        "json" => OutputFormat::Json,
        "text" => OutputFormat::Text,
        "markdown" | "md" => OutputFormat::Markdown,
        other => return Err(bad_request(format!("unknown format '{other}'"))),
    };

    let mut config = LiteParseConfig {
        output_format: format.clone(),
        quiet: true,
        ..Default::default()
    };
    if let Some(v) = p.ocr {
        config.ocr_enabled = v;
    }
    if let Some(v) = p.ocr_language {
        config.ocr_language = v;
    }
    if p.ocr_server_url.is_some() {
        config.ocr_server_url = p.ocr_server_url;
    }
    if p.target_pages.is_some() {
        config.target_pages = p.target_pages;
    }
    if let Some(v) = p.max_pages {
        config.max_pages = v;
    }
    if let Some(v) = p.dpi {
        config.dpi = v;
    }
    if p.password.is_some() {
        config.password = p.password;
    }

    let _permit = state.permits.acquire().await.map_err(|e| {
        ApiError(StatusCode::SERVICE_UNAVAILABLE, e.to_string())
    })?;

    let lp = LiteParse::new(config);
    let result = lp
        .parse_input(PdfInput::Bytes(data))
        .await
        .map_err(|e| ApiError(StatusCode::UNPROCESSABLE_ENTITY, e.to_string()))?;

    let (ctype, body) = match format {
        OutputFormat::Json => (
            "application/json",
            json::format_json_result(&result, lp.config().extract_text_metadata)
                .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?,
        ),
        OutputFormat::Text => ("text/plain; charset=utf-8", text::format_text(&result.pages)),
        OutputFormat::Markdown => ("text/markdown; charset=utf-8", result.text.clone()),
    };
    Ok(([(header::CONTENT_TYPE, ctype)], body).into_response())
}

#[tokio::main]
async fn main() {
    let port = env::var("PORT").unwrap_or_else(|_| "8080".into());
    let max_concurrent = env::var("MAX_CONCURRENT_PARSES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2usize);
    let max_body_mb = env::var("MAX_UPLOAD_MB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50usize);
    let api_key = env::var("API_KEY").ok().filter(|k| !k.is_empty()).map(Arc::from);
    if api_key.is_none() {
        eprintln!("[liteparse-server] WARNING: API_KEY not set, /parse is unauthenticated");
    }

    let state = AppState {
        permits: Arc::new(Semaphore::new(max_concurrent)),
        api_key,
    };
    let app = Router::new()
        .route("/parse", post(parse))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_key))
        .route("/health", get(health))
        .layer(DefaultBodyLimit::max(max_body_mb * 1024 * 1024))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .expect("bind failed");
    eprintln!("[liteparse-server] listening on :{port}");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .expect("server error");
}
