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
        60,
        96 * 1024 * 1024,
        512 * 1024 * 1024,
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

    let settings = application
        .clone()
        .oneshot(Request::get("/v1/cameras/kitchen/settings").body(Body::empty())?)
        .await?;
    assert_eq!(settings.status(), StatusCode::UNAUTHORIZED);

    let zones = application
        .clone()
        .oneshot(Request::get("/v1/cameras/kitchen/zones").body(Body::empty())?)
        .await?;
    assert_eq!(zones.status(), StatusCode::UNAUTHORIZED);

    let recordings = application
        .clone()
        .oneshot(Request::get("/v1/recordings").body(Body::empty())?)
        .await?;
    assert_eq!(recordings.status(), StatusCode::UNAUTHORIZED);

    let recordings = application
        .clone()
        .oneshot(
            Request::get("/v1/recordings")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(recordings.status(), StatusCode::OK);

    let unknown_camera = application
        .clone()
        .oneshot(
            Request::post("/v1/cameras/kitchen/recordings")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"duration_seconds":15,"request_id":"00000000-0000-4000-8000-000000000001"}"#,
                ))?,
        )
        .await?;
    assert_eq!(unknown_camera.status(), StatusCode::NOT_FOUND);

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
