use axum::{
    Json, Router,
    extract::{Path, State},
    http::HeaderMap,
    routing::get,
};

use crate::{
    blink_zone_model::{CameraZones, CameraZonesUpdate},
    error::EngineError,
    hub::EngineState,
};

pub fn routes() -> Router<EngineState> {
    Router::new().route(
        "/v1/cameras/{alias}/zones",
        get(get_zones).post(update_zones),
    )
}

async fn get_zones(
    State(state): State<EngineState>,
    headers: HeaderMap,
    Path(alias): Path<String>,
) -> Result<Json<CameraZones>, EngineError> {
    super::api::authorize(&state, &headers)?;
    super::api::validate_alias(&alias)?;
    Ok(Json(state.client().camera_zones_state(&alias).await?))
}

async fn update_zones(
    State(state): State<EngineState>,
    headers: HeaderMap,
    Path(alias): Path<String>,
    Json(input): Json<CameraZonesUpdate>,
) -> Result<Json<CameraZones>, EngineError> {
    super::api::authorize(&state, &headers)?;
    super::api::validate_alias(&alias)?;
    Ok(Json(
        state.client().update_camera_zones(&alias, &input).await?,
    ))
}
