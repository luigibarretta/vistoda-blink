//! Cached motion state and motion-recording settings (no Blink calls on GET).

use axum::{
    Json, Router,
    extract::{Path, State},
    http::HeaderMap,
    response::Response,
    routing::get,
};
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

async fn thumbnail(
    State(state): State<EngineState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, EngineError> {
    authorize(&state, &headers)?;
    if id.is_empty() || id.len() > 20 || !id.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(EngineError::RecordingNotFound);
    }
    let path = state
        .motion()
        .thumbnail(&id)
        .await
        .ok_or(EngineError::RecordingNotFound)?;
    let image = state.client().event_thumbnail(&path).await?;
    Ok(media_response(image, "image/jpeg"))
}

async fn motion(
    State(state): State<EngineState>,
    headers: HeaderMap,
) -> Result<Json<Value>, EngineError> {
    authorize(&state, &headers)?;
    let mut cameras = Vec::new();
    for camera in state.client().state().await.cameras {
        let latest = state.motion().camera(&camera.id, &camera.name).await;
        let thumbnail_available = match &latest {
            Some((motion, _)) => state.motion().thumbnail(&motion.event_id).await.is_some(),
            None => false,
        };
        cameras.push(json!({
            "alias": camera.alias,
            "name": camera.name,
            "motion_detected": latest.as_ref().is_some_and(|(_, active)| *active),
            "last_motion_at": latest.as_ref().map(|(motion, _)| motion.last_motion_at.clone()),
            "event_type": latest.as_ref().and_then(|(motion, _)| motion.event_type.clone()),
            "has_media": latest.as_ref().map(|(motion, _)| motion.has_media),
            "event_id": latest.as_ref().map(|(motion, _)| motion.event_id.clone()),
            "thumbnail_available": thumbnail_available,
        }));
    }
    Ok(Json(
        json!({ "poller": state.motion().status().await, "cameras": cameras }),
    ))
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
