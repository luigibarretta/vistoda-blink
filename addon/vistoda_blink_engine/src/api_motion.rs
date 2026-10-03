//! Cached motion state and motion-recording settings (no Blink calls on GET).

use axum::{Json, Router, extract::State, http::HeaderMap, routing::get};
use serde_json::{Value, json};

use crate::{
    api::authorize, error::EngineError, hub::EngineState, motion_settings::MotionRecordingSettings,
};

pub fn routes() -> Router<EngineState> {
    Router::new()
        .route("/v1/motion", get(motion))
        .route("/v1/motion/recording", get(settings).put(update_settings))
}

async fn motion(
    State(state): State<EngineState>,
    headers: HeaderMap,
) -> Result<Json<Value>, EngineError> {
    authorize(&state, &headers)?;
    let mut cameras = Vec::new();
    for camera in state.client().state().await.cameras {
        let latest = state.motion().camera(&camera.id, &camera.name).await;
        cameras.push(json!({
            "alias": camera.alias,
            "name": camera.name,
            "motion_detected": latest.as_ref().is_some_and(|(_, active)| *active),
            "last_motion_at": latest.as_ref().map(|(motion, _)| motion.last_motion_at.clone()),
            "event_type": latest.as_ref().and_then(|(motion, _)| motion.event_type.clone()),
            "has_media": latest.as_ref().map(|(motion, _)| motion.has_media),
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
