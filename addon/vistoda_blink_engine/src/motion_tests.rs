use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use super::{MotionTracker, SEEN_LIMIT};

type TestResult = Result<(), &'static str>;
use crate::blink_media_v4::MotionEvent;

fn event(id: &str, device: &str, seconds_ago: i64) -> MotionEvent {
    MotionEvent {
        id: id.into(),
        created_at: (OffsetDateTime::now_utc() - time::Duration::seconds(seconds_ago))
            .format(&Rfc3339)
            .unwrap_or_default(),
        device_id: Some(device.into()),
        device_name: format!("Camera {device}"),
        network_id: Some("1".into()),
        event_type: Some("pir".into()),
        has_media: false,
        no_media_reason: Some("no_subscription".into()),
        thumbnail: None,
    }
}

#[tokio::test]
async fn a_thumbnail_published_after_the_event_is_still_recorded() -> TestResult {
    let tracker = MotionTracker::default();
    tracker.absorb(vec![event("7", "11", 5)]).await;
    assert!(tracker.thumbnail("7").await.is_none());
    let mut later = event("7", "11", 5);
    later.thumbnail = Some("/api/v2/thumb/7".into());
    tracker.absorb(vec![later]).await;
    assert_eq!(
        tracker.thumbnail("7").await.as_deref(),
        Some("/api/v2/thumb/7")
    );
    let (motion, _) = tracker
        .camera("11", "Camera 11")
        .await
        .ok_or("camera state")?;
    assert_eq!(motion.event_id, "7");
    Ok(())
}

#[tokio::test]
async fn warm_up_poll_never_triggers_backlog_recordings() -> TestResult {
    let tracker = MotionTracker::default();
    let fresh = tracker.absorb(vec![event("1", "11", 30)]).await;
    assert!(
        fresh.is_empty(),
        "events seen at startup are history, not triggers"
    );
    let (motion, active) = tracker
        .camera("11", "Camera 11")
        .await
        .ok_or("camera state")?;
    assert!(active && !motion.has_media);
    Ok(())
}

#[tokio::test]
async fn new_events_trigger_once_and_duplicates_are_ignored() {
    let tracker = MotionTracker::default();
    tracker.absorb(Vec::new()).await;
    let fresh = tracker
        .absorb(vec![event("2", "11", 5), event("2", "11", 5)])
        .await;
    assert_eq!(fresh.len(), 1);
    assert!(tracker.absorb(vec![event("2", "11", 5)]).await.is_empty());
}

#[tokio::test]
async fn motion_window_expires_and_keeps_the_latest_event() -> TestResult {
    let tracker = MotionTracker::default();
    tracker
        .absorb(vec![event("3", "12", 600), event("4", "12", 300)])
        .await;
    let (motion, active) = tracker
        .camera("12", "Camera 12")
        .await
        .ok_or("camera state")?;
    assert!(
        !active,
        "a five-minute-old event is outside the motion window"
    );
    assert!(motion.last_motion_at > event("x", "12", 450).created_at);
    assert!(tracker.camera("99", "Unknown").await.is_none());
    Ok(())
}

#[tokio::test]
async fn seen_ids_stay_bounded() {
    let tracker = MotionTracker::default();
    let events = (0..SEEN_LIMIT + 10)
        .map(|index| event(&index.to_string(), "13", 1_000))
        .collect();
    tracker.absorb(events).await;
    let inner = tracker.inner.read().await;
    assert_eq!(inner.seen.len(), SEEN_LIMIT);
    assert_eq!(inner.order.len(), SEEN_LIMIT);
}

#[tokio::test]
async fn new_events_and_late_thumbnails_bump_the_long_poll_sequence() {
    let tracker = MotionTracker::default();
    let start = tracker.sequence();
    tracker.absorb(vec![event("20", "14", 5)]).await;
    let after_event = tracker.sequence();
    assert!(after_event > start);
    tracker.absorb(vec![event("20", "14", 5)]).await;
    assert_eq!(
        tracker.sequence(),
        after_event,
        "a repeated poll changes nothing"
    );
    let mut later = event("20", "14", 5);
    later.thumbnail = Some("/api/v2/thumb/20".into());
    tracker.absorb(vec![later]).await;
    assert!(tracker.sequence() > after_event);
}
