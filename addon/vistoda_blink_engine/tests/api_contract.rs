use std::error::Error;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use tower::ServiceExt;
use vistoda_blink_engine::{EngineState, router};
use zeroize::Zeroizing;

#[tokio::test]
async fn health_is_public_but_provider_state_requires_the_workload_token()
-> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let token = "a".repeat(64);
    let state = EngineState::new(
        Zeroizing::new(token.clone()),
        directory.path().join("provider.sealed"),
    )?;
    let application = router(state);

    let health = application
        .clone()
        .oneshot(Request::get("/healthz").body(Body::empty())?)
        .await?;
    assert_eq!(health.status(), StatusCode::OK);

    let unauthorized = application
        .clone()
        .oneshot(Request::get("/v1/state").body(Body::empty())?)
        .await?;
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let status = application
        .oneshot(
            Request::get("/v1/enrollment/status")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(status.status(), StatusCode::OK);
    Ok(())
}
