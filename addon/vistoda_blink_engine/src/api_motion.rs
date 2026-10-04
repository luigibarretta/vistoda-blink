//! Cached motion state and motion-recording settings (no Blink calls on GET).

use std::time::Duration;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::HeaderMap,
    response::Response,
    routing::get,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    api::{authorize, media_response},
    error::EngineError,
    hub::EngineState,
    motion_settings::MotionRecordingSettings,
};

pub fn routes() -> Router<EngineState> {
    Router::new()
        .route("/v1/motion", get(motion))
        .route("/v1/motion/recording", get(settings).put(update_settings))
        .route("/v1/motion/events/{id}/thumbnail.jpg", get(thumbnail))
}

/// Optional long-poll: `?since=<sequence>&wait=<seconds>` returns as soon as
/// the motion state changes, or after at most 30 seconds (ADR 0015).
#[derive(Deserialize)]
struct MotionQuery {
    since: Option<u64>,
    wait: Option<u64>,
}

async fn thumbnail(
    State(state): State<EngineState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, EngineError> {
    authorize(&state, &headers)?;
    if id.is_empty() || id.len() > 20 || !id.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(EngineError::RecordingNotFound);
    }
    // Blink's own still shows the trigger moment; the local one is a fallback.
    if let Some(path) = state.motion().thumbnail(&id).await {
        match state.client().event_thumbnail(&path).await {
            Ok(image) => return Ok(media_response(image, "image/jpeg")),
            Err(error) => {
                let Some(still) = state.motion().local_still(&id).await else {
                    return Err(error.into());
                };
                return Ok(media_response(still, "image/jpeg"));
            }
        }
    }
    let still = state
        .motion()
        .local_still(&id)
        .await
        .ok_or(EngineError::RecordingNotFound)?;
    Ok(media_response(still, "image/jpeg"))
}

async fn motion(
    State(state): State<EngineState>,
    headers: HeaderMap,
    Query(query): Query<MotionQuery>,
) -> Result<Json<Value>, EngineError> {
    authorize(&state, &headers)?;
    if let (Some(since), Some(wait)) = (query.since, query.wait) {
        state
            .motion()
            .wait_for_change(since, Duration::from_secs(wait))
            .await;
    }
    // Read the sequence first: a change during the build is seen next time.
    let sequence = state.motion().sequence();
    let mut cameras = Vec::new();
    for camera in state.client().state().await.cameras {
        let latest = state.motion().camera(&camera.id, &camera.name).await;
        let source = match &latest {
            Some((motion, _)) => thumbnail_source(&state, &motion.event_id).await,
            None => None,
        };
        cameras.push(json!({
            "alias": camera.alias,
            "name": camera.name,
            "motion_detected": latest.as_ref().is_some_and(|(_, active)| *active),
            "last_motion_at": latest.as_ref().map(|(motion, _)| motion.last_motion_at.clone()),
            "event_type": latest.as_ref().and_then(|(motion, _)| motion.event_type.clone()),
            "has_media": latest.as_ref().map(|(motion, _)| motion.has_media),
            "event_id": latest.as_ref().map(|(motion, _)| motion.event_id.clone()),
            "thumbnail_available": source.is_some(),
            "thumbnail_source": source,
        }));
    }
    Ok(Json(json!({
        "poller": state.motion().status().await,
        "sequence": sequence,
        "cameras": cameras,
    })))
}

async fn thumbnail_source(state: &EngineState, event_id: &str) -> Option<&'static str> {
    if state.motion().thumbnail(event_id).await.is_some() {
        Some("blink")
    } else if state.motion().local_still(event_id).await.is_some() {
        Some("local")
    } else {
        None
    }
}

async fn settings(
    State(state): State<EngineState>,
    headers: HeaderMap,
) -> Result<Json<Value>, EngineError> {
    authorize(&state, &headers)?;
    Ok(Json(response(&state).await))
}

async fn update_settings(
    State(state): State<EngineState>,
    headers: HeaderMap,
    Json(request): Json<MotionRecordingSettings>,
) -> Result<Json<Value>, EngineError> {
    authorize(&state, &headers)?;
    request.validate()?;
    let known = state.client().state().await.cameras;
    if !request
        .cameras
        .iter()
        .all(|alias| known.iter().any(|camera| &camera.alias == alias))
    {
        return Err(EngineError::CameraNotFound);
    }
    state.motion_settings().save(&request)?;
    Ok(Json(response(&state).await))
}

async fn response(state: &EngineState) -> Value {
    let cameras: Vec<Value> = state
        .client()
        .state()
        .await
        .cameras
        .into_iter()
        .map(
            |camera| json!({"alias": camera.alias, "name": camera.name, "powered": camera.powered}),
        )
        .collect();
    json!({
        "settings": state.motion_settings().load(),
        "cameras": cameras,
        "poller": state.motion().status().await,
    })
}

#[cfg(test)]
#[path = "api_motion_tests.rs"]
mod tests;
