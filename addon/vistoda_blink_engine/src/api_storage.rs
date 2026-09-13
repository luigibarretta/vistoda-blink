use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::HeaderMap,
    response::Response,
    routing::{delete, get, post},
};

use crate::{
    api::{authorize, media_response},
    blink_storage::LocalStorageInventory,
    error::EngineError,
    hub::EngineState,
};
use serde::Deserialize;
use std::collections::BTreeSet;

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoragePage {
    page: Option<usize>,
    page_size: Option<usize>,
    cameras: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FormatRequest {
    confirmation: String,
}

pub fn routes() -> Router<EngineState> {
    Router::new()
        .route("/v1/local-storage", get(list))
        .route(
            "/v1/local-storage/{network}/{sync}/{manifest}/{clip}/media",
            get(media),
        )
        .route(
            "/v1/local-storage/{network}/{sync}/{manifest}/{clip}",
            delete(delete_clip),
        )
        .route(
            "/v1/local-storage/{network}/{sync}/format",
            post(format_storage),
        )
}

async fn list(
    State(state): State<EngineState>,
    headers: HeaderMap,
    Query(query): Query<StoragePage>,
) -> Result<Json<serde_json::Value>, EngineError> {
    authorize(&state, &headers)?;
    let cameras = query
        .cameras
        .as_deref()
        .map(parse_camera_filter)
        .transpose()?;
    let storages: Vec<LocalStorageInventory> = state
        .client()
        .local_storage_inventories(query.page, query.page_size, cameras.as_ref())
        .await?;
    Ok(Json(serde_json::json!({"storages": storages})))
}

fn parse_camera_filter(value: &str) -> Result<BTreeSet<String>, EngineError> {
    let cameras = serde_json::from_str::<Vec<String>>(value)
        .map_err(|_| EngineError::InvalidStorageOperation)?
        .into_iter()
        .collect::<BTreeSet<_>>();
    if cameras.is_empty() || cameras.len() > 64 || cameras.iter().any(|name| name.len() > 255) {
        return Err(EngineError::InvalidStorageOperation);
    }
    Ok(cameras)
}

async fn media(
    State(state): State<EngineState>,
    headers: HeaderMap,
    Path((network, sync, manifest, clip)): Path<(u64, u64, u64, u64)>,
) -> Result<Response, EngineError> {
    authorize(&state, &headers)?;
    Ok(media_response(
        state
            .client()
            .local_storage_clip(network, sync, manifest, clip)
            .await?,
        "video/mp4",
    ))
}

async fn delete_clip(
    State(state): State<EngineState>,
    headers: HeaderMap,
    Path((network, sync, manifest, clip)): Path<(u64, u64, u64, u64)>,
) -> Result<axum::http::StatusCode, EngineError> {
    authorize(&state, &headers)?;
    state
        .client()
        .delete_local_storage_clip(network, sync, manifest, clip)
        .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

async fn format_storage(
    State(state): State<EngineState>,
    headers: HeaderMap,
    Path((network, sync)): Path<(u64, u64)>,
    Json(request): Json<FormatRequest>,
) -> Result<axum::http::StatusCode, EngineError> {
    authorize(&state, &headers)?;
    if request.confirmation != format!("FORMATTA {network}/{sync}") {
        return Err(EngineError::InvalidStorageOperation);
    }
    state.client().format_local_storage(network, sync).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::parse_camera_filter;

    #[test]
    fn destructive_routes_are_explicit_and_bounded() {
        let source = include_str!("api_storage.rs");
        let production_source = source.split("#[cfg(test)]").next().unwrap_or(source);
        assert!(production_source.contains("delete(delete_clip)"));
        assert!(production_source.contains("post(format_storage)"));
        assert!(!production_source.contains("/eject"));
        assert!(!production_source.contains("/mount"));
    }

    #[test]
    fn camera_filter_preserves_names_and_rejects_invalid_payloads() {
        let cameras =
            parse_camera_filter(r#"["Cucina, interna","Balcone"]"#).expect("valid camera filter");
        assert!(cameras.contains("Cucina, interna"));
        assert!(cameras.contains("Balcone"));
        assert!(parse_camera_filter("[]").is_err());
        assert!(parse_camera_filter("not-json").is_err());
    }
}
