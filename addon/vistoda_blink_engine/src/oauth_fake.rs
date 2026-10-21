//! In-process fake of Blink's OAuth server for the sign-in, 2FA and refresh
//! tests. Plain HTTP on loopback; no Blink account or network is involved.
// Axum handlers must be async even when they never await.
#![allow(clippy::unused_async)]

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use axum::{
    Form, Json, Router,
    extract::{RawQuery, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::json;

pub const CSRF: &str = "fake-csrf";
pub const CODE: &str = "fake-code";
pub const TWO_FACTOR_USER: &str = "2fa@example.com";
pub const BAD_USER: &str = "bad@example.com";
pub const TWO_FACTOR_CODE: &str = "123456";

#[derive(Clone, Copy)]
pub enum TokenReply {
    /// Issue `access-N` and the rotated `rotated-N`.
    Rotate,
    Status(u16, &'static str),
    /// Never answer within the OAuth timeout.
    Stall,
}

pub struct FakeState {
    pub refresh_calls: AtomicUsize,
    pub refresh_tokens: Mutex<Vec<String>>,
    reply: Mutex<TokenReply>,
    delay: Mutex<Duration>,
}

pub struct FakeOAuth {
    pub base: String,
    pub state: Arc<FakeState>,
}

impl FakeOAuth {
    pub async fn start(reply: TokenReply) -> std::io::Result<Self> {
        let state = Arc::new(FakeState {
            refresh_calls: AtomicUsize::new(0),
            refresh_tokens: Mutex::new(Vec::new()),
            reply: Mutex::new(reply),
            delay: Mutex::new(Duration::ZERO),
        });
        let app = Router::new()
            .route("/oauth/v2/authorize", get(authorize))
            .route("/oauth/v2/signin", get(sign_in_page).post(sign_in))
            .route("/oauth/v2/2fa/verify", post(verify))
            .route("/oauth/token", post(token))
            .with_state(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let base = format!("http://{}", listener.local_addr()?);
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Ok(Self { base, state })
    }

    pub fn calls(&self) -> usize {
        self.state.refresh_calls.load(Ordering::SeqCst)
    }

    pub fn set_delay(&self, delay: Duration) {
        *lock(&self.state.delay) = delay;
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The browser's first request carries the PKCE query; the final one does not.
async fn authorize(RawQuery(query): RawQuery) -> Response {
    if query.is_some_and(|value| value.contains("code_challenge=")) {
        return "authorize".into_response();
    }
    let location = format!("immedia-blink://applinks.blink.com/signin/callback?code={CODE}");
    (StatusCode::FOUND, [(header::LOCATION, location)]).into_response()
}

async fn sign_in_page() -> String {
    format!(r#"<script id="oauth-args" type="application/json">{{"csrf-token":"{CSRF}"}}</script>"#)
}

async fn sign_in(Form(form): Form<HashMap<String, String>>) -> StatusCode {
    match form.get("username").map(String::as_str) {
        _ if form.get("csrf-token").map(String::as_str) != Some(CSRF) => StatusCode::FORBIDDEN,
        Some(TWO_FACTOR_USER) => StatusCode::PRECONDITION_FAILED,
        Some(BAD_USER) => StatusCode::UNAUTHORIZED,
        _ => StatusCode::FOUND,
    }
}

async fn verify(Form(form): Form<HashMap<String, String>>) -> Response {
    if form.get("2fa_code").map(String::as_str) == Some(TWO_FACTOR_CODE) {
        (
            StatusCode::CREATED,
            Json(json!({"status": "auth-completed"})),
        )
            .into_response()
    } else {
        (StatusCode::UNAUTHORIZED, Json(json!({"status": "invalid"}))).into_response()
    }
}

async fn token(
    State(state): State<Arc<FakeState>>,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    if form.get("grant_type").map(String::as_str) == Some("authorization_code") {
        if form.get("code").map(String::as_str) != Some(CODE)
            || form.get("code_verifier").is_none_or(String::is_empty)
        {
            return StatusCode::BAD_REQUEST.into_response();
        }
        return tokens("access-0", "refresh-0");
    }
    let call = state.refresh_calls.fetch_add(1, Ordering::SeqCst) + 1;
    lock(&state.refresh_tokens).push(form.get("refresh_token").cloned().unwrap_or_default());
    let delay = *lock(&state.delay);
    tokio::time::sleep(delay).await;
    let reply = *lock(&state.reply);
    match reply {
        TokenReply::Rotate => tokens(&format!("access-{call}"), &format!("rotated-{call}")),
        TokenReply::Status(status, body) => (
            StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            body.to_owned(),
        )
            .into_response(),
        TokenReply::Stall => {
            tokio::time::sleep(Duration::from_secs(30)).await;
            StatusCode::GATEWAY_TIMEOUT.into_response()
        }
    }
}

fn tokens(access: &str, refresh: &str) -> Response {
    Json(json!({"access_token": access, "refresh_token": refresh, "expires_in": 3600}))
        .into_response()
}
