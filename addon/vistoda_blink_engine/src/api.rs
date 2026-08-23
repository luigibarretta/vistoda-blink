use std::convert::Infallible;

use axum::{
    Json, Router,
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use bytes::Bytes;
use serde::Deserialize;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

use crate::{
    auth::require_bearer,
    blink_api::CameraAction,
    enrollment::{CompleteRequest, EnrollmentOutcome, ImportRequest, StartRequest},
    error::EngineError,
    hub::{EngineState, HubMessage},
};

pub fn router(state: EngineState) -> Router {
    Router::new()
        .route("/healthz", get(health))
        .route("/metrics", get(metrics))
        .route("/v1/enrollment/status", get(enrollment_status))
        .route("/v1/enrollment/start", post(enrollment_start))
        .route("/v1/enrollment/{id}/complete", post(enrollment_complete))
        .route("/v1/enrollment/import", post(enrollment_import))
        .route("/v1/state", get(provider_state))
        .route("/v1/refresh", post(refresh))
        .route("/v1/cameras", get(cameras))
        .route("/v1/cameras/{alias}/snapshot.jpg", get(snapshot))
        .route("/v1/cameras/{alias}/live.ts", get(live))
        .route("/v1/cameras/{alias}/live.mpegts", get(live))
        .route("/v1/cameras/{alias}/commands", post(camera_command))
        .route("/v1/networks/{id}/armed", post(network_armed))
        .route("/v1/clips/{id}", get(clip))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}

async fn health(State(state): State<EngineState>) -> Json<crate::engine_metrics::Health> {
    Json(state.health().await)
}

async fn metrics(
    State(state): State<EngineState>,
    headers: HeaderMap,
) -> Result<Response, EngineError> {
    authorize(&state, &headers)?;
    Ok((
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        state.metrics().await,
    )
        .into_response())
}

async fn enrollment_status(
    State(state): State<EngineState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, EngineError> {
    authorize(&state, &headers)?;
    Ok(Json(
        serde_json::json!({"enrolled": state.client().enrolled().await}),
    ))
}

async fn enrollment_start(
    State(state): State<EngineState>,
    headers: HeaderMap,
    Json(input): Json<StartRequest>,
) -> Result<Json<EnrollmentOutcome>, EngineError> {
    authorize(&state, &headers)?;
    Ok(Json(state.enrollment().start(input).await?))
}

async fn enrollment_complete(
    State(state): State<EngineState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(input): Json<CompleteRequest>,
) -> Result<Json<EnrollmentOutcome>, EngineError> {
    authorize(&state, &headers)?;
    Ok(Json(state.enrollment().complete(id, input).await?))
}

async fn enrollment_import(
    State(state): State<EngineState>,
    headers: HeaderMap,
    Json(input): Json<ImportRequest>,
) -> Result<Json<EnrollmentOutcome>, EngineError> {
    authorize(&state, &headers)?;
    Ok(Json(state.enrollment().import(input).await?))
}

async fn provider_state(
    State(state): State<EngineState>,
    headers: HeaderMap,
) -> Result<Json<crate::blink_model::ProviderState>, EngineError> {
    authorize(&state, &headers)?;
    Ok(Json(state.client().state().await))
}

async fn refresh(
    State(state): State<EngineState>,
    headers: HeaderMap,
) -> Result<StatusCode, EngineError> {
    authorize(&state, &headers)?;
    state.client().refresh_state().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn cameras(
    State(state): State<EngineState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, EngineError> {
    authorize(&state, &headers)?;
    Ok(Json(
        serde_json::json!({"cameras": state.client().state().await.cameras}),
    ))
}

async fn snapshot(
    State(state): State<EngineState>,
    headers: HeaderMap,
    Path(alias): Path<String>,
) -> Result<Response, EngineError> {
    authorize(&state, &headers)?;
    validate_alias(&alias)?;
    Ok(media_response(
        state.client().snapshot(&alias).await?,
        "image/jpeg",
    ))
}

async fn live(
    State(state): State<EngineState>,
    headers: HeaderMap,
    Path(alias): Path<String>,
) -> Result<Response, EngineError> {
    authorize(&state, &headers)?;
    validate_alias(&alias)?;
    let mut subscriber = state.subscribe(&alias).await?;
    let stream = async_stream::stream! {
        loop {
            match subscriber.recv().await {
                Ok(HubMessage::Data(frame)) => yield Ok::<Bytes, Infallible>(frame),
                Ok(HubMessage::End) | Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
            }
        }
    };
    Ok((
        StatusCode::OK,
        media_headers("video/mp2t"),
        Body::from_stream(stream),
    )
        .into_response())
}

#[derive(Deserialize)]
struct CameraCommand {
    action: String,
    enabled: Option<bool>,
}

async fn camera_command(
    State(state): State<EngineState>,
    headers: HeaderMap,
    Path(alias): Path<String>,
    Json(input): Json<CameraCommand>,
) -> Result<StatusCode, EngineError> {
    authorize(&state, &headers)?;
    validate_alias(&alias)?;
    let action = match input.action.as_str() {
        "motion" => CameraAction::Motion(input.enabled.ok_or(EngineError::InvalidAlias)?),
        "record" => CameraAction::Record,
        "snapshot" => CameraAction::Snapshot,
        _ => return Err(EngineError::InvalidAlias),
    };
    state.client().camera_command(&alias, action).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct ArmRequest {
    armed: bool,
}

async fn network_armed(
    State(state): State<EngineState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<ArmRequest>,
) -> Result<StatusCode, EngineError> {
    authorize(&state, &headers)?;
    state.client().set_armed(&id, input.armed).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn clip(
    State(state): State<EngineState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, EngineError> {
    authorize(&state, &headers)?;
    Ok(media_response(state.client().clip(&id).await?, "video/mp4"))
}

fn authorize(state: &EngineState, headers: &HeaderMap) -> Result<(), EngineError> {
    require_bearer(headers, state.token())
}

fn media_response(content: Bytes, content_type: &'static str) -> Response {
    (StatusCode::OK, media_headers(content_type), content).into_response()
}

const fn media_headers(content_type: &'static str) -> [(header::HeaderName, &'static str); 3] {
    [
        (header::CONTENT_TYPE, content_type),
        (header::CACHE_CONTROL, "no-store"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ]
}

fn validate_alias(alias: &str) -> Result<(), EngineError> {
    if !(1..=64).contains(&alias.len())
        || !alias.bytes().all(|value| {
            value.is_ascii_lowercase() || value.is_ascii_digit() || b"_-".contains(&value)
        })
    {
        return Err(EngineError::InvalidAlias);
    }
    Ok(())
}
