use axum::{
    Json, Router,
    extract::{Path, State},
    http::HeaderMap,
    response::Response,
    routing::get,
};

use crate::{
    api::{authorize, media_response},
    blink_storage::LocalStorageInventory,
    error::EngineError,
    hub::EngineState,
};

pub fn routes() -> Router<EngineState> {
    Router::new().route("/v1/local-storage", get(list)).route(
        "/v1/local-storage/{network}/{sync}/{manifest}/{clip}/media",
        get(media),
    )
}

async fn list(
    State(state): State<EngineState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, EngineError> {
    authorize(&state, &headers)?;
    let storages: Vec<LocalStorageInventory> = state.client().local_storage_inventories().await?;
    Ok(Json(serde_json::json!({"storages": storages})))
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

#[cfg(test)]
mod tests {
    #[test]
    fn source_exposes_no_destructive_storage_route() {
        let source = include_str!("api_storage.rs");
        for fragments in [
            ["del", "ete("],
            ["ej", "ect"],
            ["for", "mat"],
            ["mo", "unt"],
        ] {
            assert!(!source.contains(&fragments.concat()));
        }
    }
}
