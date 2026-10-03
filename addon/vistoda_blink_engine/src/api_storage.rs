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
    blink_storage_mutations::StorageCommand,
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
        .route("/v1/local-storage/status", get(statuses))
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
        .route("/v1/local-storage/{network}/{sync}/eject", post(eject))
        .route("/v1/local-storage/{network}/{sync}/mount", post(mount))
}

async fn statuses(
    State(state): State<EngineState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, EngineError> {
    authorize(&state, &headers)?;
    let storages = state.client().local_storage_statuses().await?;
    Ok(Json(serde_json::json!({"storages": storages})))
}

async fn eject(
    State(state): State<EngineState>,
    headers: HeaderMap,
    Path((network, sync)): Path<(u64, u64)>,
) -> Result<axum::http::StatusCode, EngineError> {
    run_command(&state, &headers, network, sync, StorageCommand::Eject).await
}

async fn mount(
    State(state): State<EngineState>,
    headers: HeaderMap,
    Path((network, sync)): Path<(u64, u64)>,
) -> Result<axum::http::StatusCode, EngineError> {
    run_command(&state, &headers, network, sync, StorageCommand::Mount).await
}

/// Eject and mount are the reversible native pair: no typed confirmation,
/// matching the official app, but still revalidated against fresh status.
async fn run_command(
    state: &EngineState,
    headers: &HeaderMap,
    network: u64,
    sync: u64,
    command: StorageCommand,
) -> Result<axum::http::StatusCode, EngineError> {
    authorize(state, headers)?;
    state
        .client()
        .local_storage_command(network, sync, command)
        .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
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
        assert!(production_source.contains("{sync}/eject\", post(eject)"));
        assert!(production_source.contains("{sync}/mount\", post(mount)"));
        assert!(!production_source.contains("delete_all"));
        assert!(!production_source.contains("change_wifi"));
    }

    #[test]
    fn camera_filter_preserves_names_and_rejects_invalid_payloads() -> Result<(), super::EngineError>
    {
        let cameras = parse_camera_filter(r#"["Cucina, interna","Balcone"]"#)?;
        assert!(cameras.contains("Cucina, interna"));
        assert!(cameras.contains("Balcone"));
        assert!(parse_camera_filter("[]").is_err());
        assert!(parse_camera_filter("not-json").is_err());
        Ok(())
    }
}
