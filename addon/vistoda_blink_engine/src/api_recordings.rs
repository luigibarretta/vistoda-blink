use std::convert::Infallible;

use axum::{
    Json, Router,
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use bytes::Bytes;
use serde::Deserialize;
use tokio::io::AsyncReadExt;
use uuid::Uuid;

use crate::{api::authorize, error::EngineError, hub::EngineState};

pub fn routes() -> Router<EngineState> {
    Router::new()
        .route("/v1/recordings", get(list_recordings))
        .route(
            "/v1/recordings/{recording_id}",
            get(get_recording).delete(delete_recording),
        )
        .route(
            "/v1/recordings/{recording_id}/media",
            get(download_recording),
        )
        .route("/v1/cameras/{alias}/recordings", post(create_recording))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordingRequest {
    duration_seconds: u64,
    request_id: String,
}

async fn create_recording(
    State(state): State<EngineState>,
    headers: HeaderMap,
    Path(alias): Path<String>,
    Json(input): Json<RecordingRequest>,
) -> Result<impl IntoResponse, EngineError> {
    authorize(&state, &headers)?;
    crate::api::validate_alias(&alias)?;
    let manifest = state
        .recordings()
        .start(
            state.clone(),
            &alias,
            input.duration_seconds,
            &input.request_id,
        )
        .await?;
    Ok((StatusCode::ACCEPTED, Json(manifest)))
}

async fn list_recordings(
    State(state): State<EngineState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, EngineError> {
    authorize(&state, &headers)?;
    Ok(Json(serde_json::json!({
        "recordings": state.recordings().list().await
    })))
}

async fn get_recording(
    State(state): State<EngineState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<crate::recordings::RecordingManifest>, EngineError> {
    authorize(&state, &headers)?;
    validate_id(&id)?;
    state
        .recordings()
        .get(&id)
        .await
        .map(Json)
        .ok_or(EngineError::RecordingNotFound)
}

async fn delete_recording(
    State(state): State<EngineState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, EngineError> {
    authorize(&state, &headers)?;
    validate_id(&id)?;
    if state.recordings().acknowledge(&id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(EngineError::RecordingNotFound)
    }
}

async fn download_recording(
    State(state): State<EngineState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, EngineError> {
    authorize(&state, &headers)?;
    validate_id(&id)?;
    let path = state
        .recordings()
        .media_path(&id)
        .await
        .ok_or(EngineError::RecordingNotFound)?;
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|_| EngineError::RecordingIo)?;
    let stream = async_stream::stream! {
        let mut buffer = vec![0_u8; 64 * 1024];
        loop {
            match file.read(&mut buffer).await {
                Ok(0) => break,
                Ok(read) => yield Ok::<Bytes, Infallible>(Bytes::copy_from_slice(&buffer[..read])),
                Err(_) => break,
            }
        }
    };
    let mut response = Body::from_stream(stream).into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("video/mp2t"));
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    Ok(response)
}

fn validate_id(value: &str) -> Result<(), EngineError> {
    Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| EngineError::RecordingInvalid)
}
