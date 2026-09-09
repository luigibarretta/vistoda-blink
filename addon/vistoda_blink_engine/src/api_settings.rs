use axum::{
    Json, Router,
    extract::{Path, State},
    http::HeaderMap,
    routing::get,
};

use crate::{
    blink_settings::CameraSettings, blink_settings_write::CameraSettingUpdate, error::EngineError,
    hub::EngineState,
};

pub fn routes() -> Router<EngineState> {
    Router::new().route(
        "/v1/cameras/{alias}/settings",
        get(get_settings).post(update_setting),
    )
}

async fn get_settings(
    State(state): State<EngineState>,
    headers: HeaderMap,
    Path(alias): Path<String>,
) -> Result<Json<CameraSettings>, EngineError> {
    super::api::authorize(&state, &headers)?;
    super::api::validate_alias(&alias)?;
    Ok(Json(state.client().camera_settings(&alias).await?))
}

async fn update_setting(
    State(state): State<EngineState>,
    headers: HeaderMap,
    Path(alias): Path<String>,
    Json(input): Json<CameraSettingUpdate>,
) -> Result<Json<CameraSettings>, EngineError> {
    super::api::authorize(&state, &headers)?;
    super::api::validate_alias(&alias)?;
    Ok(Json(
        state.client().update_camera_setting(&alias, &input).await?,
    ))
}
