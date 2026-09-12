use super::*;
use crate::{hub::CameraHub, immi_audio_lease::AudioLeaseError};
use bytes::Bytes;

#[test]
fn idle_viewers_do_not_claim_or_revoke_another_viewers_microphone()
-> Result<(), Box<dyn std::error::Error>> {
    let hub = Arc::new(CameraHub::new());
    let publisher = hub.acquire_publisher(1)?;
    let first = hub.subscribe();
    let second = hub.subscribe();
    let (connection, _frames) = publisher.audio().connect();
    connection.observe_offer(0xa000_0001)?;
    assert!(!second.audio().subscribe().borrow().microphone_enabled);
    let lease = first.audio().claim()?;
    assert!(matches!(second.audio().claim(), Err(AudioLeaseError::Busy)));
    drop(second);
    assert!(first.audio().subscribe().borrow().microphone_enabled);
    drop(lease);
    assert!(!first.audio().subscribe().borrow().microphone_enabled);
    assert!(first.audio().claim().is_ok());
    Ok(())
}

#[tokio::test]
async fn microphone_cancellation_revokes_lease_but_keeps_shared_video_publisher()
-> Result<(), Box<dyn std::error::Error>> {
    let hub = Arc::new(CameraHub::new());
    let publisher = hub.acquire_publisher(1)?;
    let mut video = hub.subscribe();
    let mut microphone = hub.subscribe();
    let runtime = microphone.audio();
    let (connection, _frames) = publisher.audio().connect();
    connection.observe_offer(0xa000_0001)?;
    let lease = runtime.claim()?;
    assert!(matches!(video.audio().claim(), Err(AudioLeaseError::Busy)));
    assert!(hub.acquire_publisher(1).is_err());
    let task = tokio::spawn(async move {
        let _lease = lease;
        drain(&mut microphone).await
    });
    publisher.publish(Bytes::from_static(b"first TS chunk"));
    assert!(
        matches!(video.recv().await?, HubMessage::Data(bytes) if bytes == b"first TS chunk"[..])
    );
    tokio::task::yield_now().await;
    task.abort();
    assert!(task.await.is_err());
    assert_eq!(hub.snapshot().subscribers, 1);
    assert!(publisher.has_subscribers());
    assert!(!runtime.subscribe().borrow().microphone_enabled);
    assert_eq!(runtime.subscribe().borrow().format, Some(0xa000_0001));
    publisher.publish(Bytes::from_static(b"video continues"));
    assert!(
        matches!(video.recv().await?, HubMessage::Data(bytes) if bytes == b"video continues"[..])
    );
    Ok(())
}

#[tokio::test]
async fn microphone_drain_returns_on_publisher_end() -> Result<(), Box<dyn std::error::Error>> {
    let hub = Arc::new(CameraHub::new());
    let publisher = hub.acquire_publisher(1)?;
    let mut subscriber = hub.subscribe();
    publisher.publish(Bytes::from_static(b"ignored TS"));
    drop(publisher);
    tokio::time::timeout(Duration::from_millis(100), drain(&mut subscriber)).await??;
    Ok(())
}

#[tokio::test]
async fn microphone_drain_lag_is_fail_closed() -> Result<(), Box<dyn std::error::Error>> {
    let hub = Arc::new(CameraHub::new());
    let publisher = hub.acquire_publisher(1)?;
    let mut subscriber = hub.subscribe();
    for _ in 0..32 {
        publisher.publish(Bytes::from_static(b"ignored TS"));
    }
    assert!(drain(&mut subscriber).await.is_err());
    assert!(hub.snapshot().lagged > 0);
    Ok(())
}

#[test]
fn control_only_contract_requires_bounded_microphone_request_ids() {
    for invalid in [
        r#"{"type":"ack","sequence":1}"#,
        r#"{"type":"media"}"#,
        r#"{"type":"microphone","enabled":true}"#,
        r#"{"type":"microphone","enabled":true,"request_id":4294967296}"#,
        r#"{"type":"microphone","enabled":true,"request_id":-1}"#,
    ] {
        assert!(serde_json::from_str::<Control>(invalid).is_err());
    }
    assert!(matches!(
        serde_json::from_str::<Control>(r#"{"type":"microphone","enabled":true,"request_id":72}"#),
        Ok(Control::Microphone {
            enabled: true,
            request_id: 72
        })
    ));
}
