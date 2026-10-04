//! Route contracts for the local motion still, long-poll and program writes.

use std::{error::Error, time::Duration};

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use bytes::Bytes;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;
use zeroize::Zeroizing;

use crate::{blink_media_v4::MotionEvent, blink_model::CameraState, hub::EngineState};

type TestResult = Result<(), Box<dyn Error>>;

fn token() -> String {
    "b".repeat(64)
}

async fn engine(directory: &tempfile::TempDir) -> Result<(EngineState, Router), Box<dyn Error>> {
    let state = EngineState::new(
        Zeroizing::new(token()),
        directory.path().join("provider.sealed"),
        60,
        1024 * 1024,
        8 * 1024 * 1024,
    )?;
    let camera: CameraState = serde_json::from_value(json!({
        "id": "11", "network_id": "1", "alias": "balcone", "name": "Balcone",
        "camera_type": "default", "product_type": "catalina", "motion_detected": false,
        "powered": false, "preferred_live_transport": "walnut"
    }))?;
    state.client().inner.state.write().await.cameras = vec![camera];
    Ok((state.clone(), crate::router(state)))
}

async fn get(router: &Router, uri: &str) -> Result<(StatusCode, Bytes), Box<dyn Error>> {
    let response = router
        .clone()
        .oneshot(
            Request::get(uri)
                .header(header::AUTHORIZATION, format!("Bearer {}", token()))
                .body(Body::empty())?,
        )
        .await?;
    let status = response.status();
    Ok((status, response.into_body().collect().await?.to_bytes()))
}

fn event(id: &str) -> MotionEvent {
    MotionEvent {
        id: id.into(),
        created_at: time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default(),
        device_id: Some("11".into()),
        device_name: "Balcone".into(),
        network_id: Some("1".into()),
        event_type: Some("pir".into()),
        has_media: false,
        no_media_reason: Some("no_subscription".into()),
        thumbnail: None,
    }
}

#[tokio::test]
async fn a_local_still_backs_an_event_without_blink_thumbnail() -> TestResult {
    let directory = tempfile::tempdir()?;
    let (state, router) = engine(&directory).await?;
    state.motion().absorb(vec![event("77")]).await;
    let (status, body) = get(&router, "/v1/motion").await?;
    assert_eq!(status, StatusCode::OK);
    let payload: Value = serde_json::from_slice(&body)?;
    assert_eq!(payload["cameras"][0]["thumbnail_available"], false);
    assert_eq!(
        get(&router, "/v1/motion/events/77/thumbnail.jpg").await?.0,
        StatusCode::NOT_FOUND
    );
    let since = payload["sequence"].as_u64().ok_or("sequence")?;

    // A long-poll wakes up as soon as the still is stored.
    let waiter = tokio::spawn({
        let router = router.clone();
        async move {
            get(&router, &format!("/v1/motion?since={since}&wait=20"))
                .await
                .ok()
        }
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    let jpeg = Bytes::from_static(b"\xFF\xD8\xFFlocal\xFF\xD9");
    state.motion().store_still("77", jpeg.clone()).await;
    let (status, body) = tokio::time::timeout(Duration::from_secs(5), waiter)
        .await??
        .ok_or("long-poll")?;
    assert_eq!(status, StatusCode::OK);
    let payload: Value = serde_json::from_slice(&body)?;
    assert!(payload["sequence"].as_u64() > Some(since));
    assert_eq!(payload["cameras"][0]["event_id"], "77");
    assert_eq!(payload["cameras"][0]["thumbnail_available"], true);
    assert_eq!(payload["cameras"][0]["thumbnail_source"], "local");
    assert_eq!(payload["poller"]["mode"], "", "poller has not run in tests");

    let (status, body) = get(&router, "/v1/motion/events/77/thumbnail.jpg").await?;
    assert_eq!((status, body), (StatusCode::OK, jpeg));
    Ok(())
}

#[tokio::test]
async fn an_unchanged_long_poll_times_out_and_bad_queries_are_rejected() -> TestResult {
    let directory = tempfile::tempdir()?;
    let (state, router) = engine(&directory).await?;
    let since = state.motion().sequence();
    let started = std::time::Instant::now();
    let (status, _) = get(&router, &format!("/v1/motion?since={since}&wait=1")).await?;
    assert_eq!(status, StatusCode::OK);
    assert!(started.elapsed() >= Duration::from_millis(900));
    // A stale sequence returns at once.
    let started = std::time::Instant::now();
    get(&router, &format!("/v1/motion?since={}&wait=20", since + 5)).await?;
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(
        get(&router, "/v1/motion?wait=x").await?.0,
        StatusCode::BAD_REQUEST
    );
    Ok(())
}

#[tokio::test]
async fn program_routes_validate_before_reaching_blink() -> TestResult {
    let directory = tempfile::tempdir()?;
    let (_, router) = engine(&directory).await?;
    let (status, body) = get(&router, "/v1/programs").await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        serde_json::from_slice::<Value>(&body)?,
        json!({"programs": []})
    );
    for (uri, expected) in [
        ("/v1/networks/1/programs/abc/enabled", StatusCode::NOT_FOUND),
        ("/v1/networks/999/programs/5/enabled", StatusCode::NOT_FOUND),
    ] {
        let response = router
            .clone()
            .oneshot(
                Request::post(uri)
                    .header(header::AUTHORIZATION, format!("Bearer {}", token()))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"enabled":true}"#))?,
            )
            .await?;
        assert_eq!(response.status(), expected, "{uri}");
    }
    let unauthorized = router
        .clone()
        .oneshot(
            Request::post("/v1/networks/1/programs/5/enabled")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"enabled":false}"#))?,
        )
        .await?;
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    Ok(())
}
