use std::error::Error;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;
use vistoda_blink_engine::{EngineState, router};
use zeroize::Zeroizing;

#[tokio::test]
async fn health_and_metrics_expose_session_poller_and_live_state() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let token = "b".repeat(64);
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
    let health: Value = serde_json::from_slice(&health.into_body().collect().await?.to_bytes())?;
    assert_eq!(health["status"], "ok");
    assert_eq!(health["enrolled"], false);
    assert_eq!(health["reauth_required"], false);
    assert_eq!(health["credentials_persisted"], true);

    let metrics = application
        .oneshot(
            Request::get("/metrics")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(metrics.status(), StatusCode::OK);
    let text = String::from_utf8(metrics.into_body().collect().await?.to_bytes().to_vec())?;
    for line in [
        "vistoda_blink_packets_total 0",
        "vistoda_blink_live_sessions_active 0",
        "vistoda_blink_live_subscribers 0",
        "vistoda_blink_motion_poll_failures_total 0",
        "vistoda_blink_motion_rate_limited 0",
        "vistoda_blink_motion_poller_state{state=\"starting\"} 1",
        "vistoda_blink_motion_poller_state{state=\"backoff\"} 0",
        "vistoda_blink_oauth_refreshes_total{result=\"success\"} 0",
        "vistoda_blink_oauth_refreshes_total{result=\"rejected\"} 0",
        "vistoda_blink_reauth_required 0",
        "vistoda_blink_credentials_persist_pending 0",
        "vistoda_blink_credentials_persist_failures_total 0",
    ] {
        assert!(text.lines().any(|value| value == line), "missing {line}");
    }
    Ok(())
}
