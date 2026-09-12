use super::*;
use crate::{
    hub::{CameraHub, HubMessage},
    immi_audio_lease::{AudioLeaseError, AudioStatus},
};
use std::{error::Error, sync::Arc};
use tokio::time::timeout;

const OFFER_AVAILABLE: [u8; 18] = [
    0x0c, 0xa0, 0, 0, 1, 0, 0, 0, 0, 0x18, 0, 0, 0, 4, 0, 0, 0, 0,
];
const VIDEO: [u8; 11] = [0, 0, 0, 0, 0, 0, 0, 0, 2, 0x47, 42];

#[tokio::test]
async fn mclv_drop_without_frames_stops_and_audio_wait_preserves_video()
-> Result<(), Box<dyn Error>> {
    let hub = Arc::new(CameraHub::new());
    let publisher = hub.acquire_publisher(1)?;
    let mut subscriber = hub.subscribe();
    let runtime = subscriber.audio();
    let mut status = runtime.subscribe();
    let (client, mut server) = tokio::io::duplex(1024);
    let task = tokio::spawn(async move { receive_stream(client, &publisher, Some(true)).await });
    server.write_all(&OFFER_AVAILABLE).await?;
    timeout(
        Duration::from_secs(1),
        status.wait_for(AudioStatus::supported),
    )
    .await??;
    let mut packet = [0; 9];
    assert!(
        timeout(Duration::from_millis(20), server.read_exact(&mut packet))
            .await
            .is_err()
    );
    let lease = runtime.claim()?;
    timeout(Duration::from_millis(500), server.read_exact(&mut packet)).await??;
    assert_eq!(packet, [0x17, 0, 0, 0, 3, 0, 0, 0, 0]);
    drop(lease); // No frame is necessary to wake the writer for Stop.
    timeout(Duration::from_millis(500), server.read_exact(&mut packet)).await??;
    assert_eq!(packet, [0x17, 0, 0, 0, 4, 0, 0, 0, 0]);
    let lease = runtime.claim()?;
    timeout(Duration::from_millis(500), server.read_exact(&mut packet)).await??;
    assert_eq!(packet[4], 3);
    server.write_all(&[0x18, 0, 0, 0, 5, 0, 0, 0, 0]).await?;
    timeout(Duration::from_millis(500), server.read_exact(&mut packet)).await??;
    assert_eq!(packet[4], 4);
    assert!(!status.borrow().supported());
    assert!(!status.borrow().microphone_enabled);
    assert!(matches!(runtime.claim(), Err(AudioLeaseError::Unavailable)));
    drop(lease);
    server.write_all(&VIDEO).await?;
    assert!(
        matches!(timeout(Duration::from_millis(500), subscriber.recv()).await??,
        HubMessage::Data(bytes) if bytes.as_ref() == &VIDEO[9..])
    );
    server.write_all(&OFFER_AVAILABLE[9..]).await?;
    timeout(
        Duration::from_millis(500),
        status.wait_for(AudioStatus::supported),
    )
    .await??;
    assert!(!status.borrow().microphone_enabled);
    assert!(
        timeout(Duration::from_millis(20), server.read_exact(&mut packet))
            .await
            .is_err()
    );
    let _lease = runtime.claim()?;
    timeout(Duration::from_millis(500), server.read_exact(&mut packet)).await??;
    task.abort();
    assert!(task.await.is_err());
    assert!(!status.borrow().connected);
    assert_eq!(server.read(&mut packet).await?, 0);
    Ok(())
}

#[tokio::test]
async fn partial_start_timeout_closes_tls_and_revokes_lease() -> Result<(), Box<dyn Error>> {
    let hub = Arc::new(CameraHub::new());
    let publisher = hub.acquire_publisher(1)?;
    let subscriber = hub.subscribe();
    let runtime = subscriber.audio();
    let mut status = runtime.subscribe();
    let (client, mut server) = tokio::io::duplex(1);
    let task = tokio::spawn(async move { receive_stream(client, &publisher, Some(true)).await });
    server.write_all(&OFFER_AVAILABLE).await?;
    timeout(
        Duration::from_secs(1),
        status.wait_for(AudioStatus::supported),
    )
    .await??;
    let _lease = runtime.claim()?;
    assert!(timeout(Duration::from_millis(600), task).await??.is_err());
    assert!(!status.borrow().connected);
    assert!(!status.borrow().microphone_enabled);
    let mut partial = Vec::new();
    server.read_to_end(&mut partial).await?;
    assert_eq!(partial, [0x17]);
    Ok(())
}

#[tokio::test]
async fn partial_stop_timeout_also_closes_tls() -> Result<(), Box<dyn Error>> {
    let hub = Arc::new(CameraHub::new());
    let publisher = hub.acquire_publisher(1)?;
    let subscriber = hub.subscribe();
    let runtime = subscriber.audio();
    let mut status = runtime.subscribe();
    let (client, mut server) = tokio::io::duplex(1);
    let task = tokio::spawn(async move { receive_stream(client, &publisher, Some(true)).await });
    server.write_all(&OFFER_AVAILABLE).await?;
    timeout(
        Duration::from_secs(1),
        status.wait_for(AudioStatus::supported),
    )
    .await??;
    let lease = runtime.claim()?;
    let mut start = [0; 9];
    timeout(Duration::from_millis(500), server.read_exact(&mut start)).await??;
    assert_eq!(start, [0x17, 0, 0, 0, 3, 0, 0, 0, 0]);
    drop(lease);
    assert!(timeout(Duration::from_millis(600), task).await??.is_err());
    assert!(!status.borrow().connected);
    let mut partial = Vec::new();
    server.read_to_end(&mut partial).await?;
    assert_eq!(partial, [0x17]);
    Ok(())
}
