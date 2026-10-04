//! Blink arm/disarm programs: cached list and verified enable/disable (ADR 0014).

use axum::{
    Json, Router,
    extract::{Path, State},
    http::HeaderMap,
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{api::authorize, blink_programs::Program, error::EngineError, hub::EngineState};

pub fn routes() -> Router<EngineState> {
    Router::new().route("/v1/programs", get(list)).route(
        "/v1/networks/{network}/programs/{program}/enabled",
        post(set_enabled),
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EnabledRequest {
    enabled: bool,
}

/// Served from the state cache; Blink is read at most every 10 minutes.
async fn list(
    State(state): State<EngineState>,
    headers: HeaderMap,
) -> Result<Json<Value>, EngineError> {
    authorize(&state, &headers)?;
    Ok(Json(
        json!({"programs": state.client().state().await.programs}),
    ))
}

async fn set_enabled(
    State(state): State<EngineState>,
    headers: HeaderMap,
    Path((network, program)): Path<(String, String)>,
    Json(input): Json<EnabledRequest>,
) -> Result<Json<Program>, EngineError> {
    authorize(&state, &headers)?;
    let program = state
        .client()
        .set_program_enabled(&network, &program, input.enabled)
        .await?;
    tracing::info!(
        enabled = program.enabled,
        "Blink program state changed by Vistoda"
    );
    Ok(Json(program))
}
