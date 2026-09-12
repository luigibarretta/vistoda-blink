use super::*;
use crate::{
    hub::{CameraHub, HubMessage},
    immi_audio_lease::{AudioLeaseError, AudioRuntime},
};
use std::{error::Error, sync::Arc};
use tokio::time::timeout;

fn frame() -> Vec<u8> {
    vec![0xff, 0xf1, 0x60, 0x40, 1, 0x20, 0, 0x12, 0x34]
}

#[tokio::test]
async fn live_shares_audio_offer_and_preserves_video_without_config_echo()
-> Result<(), Box<dyn Error>> {
    let hub = Arc::new(CameraHub::new());
    let publisher = hub.acquire_publisher(1)?;
    let mut subscriber = hub.subscribe();
    let runtime = subscriber.audio();
    let mut status = runtime.subscribe();
    let (client, mut server) = tokio::io::duplex(1024);
    let task = tokio::spawn(async move { receive_stream(client, &publisher).await });
    server.write_all(&[0x0c, 0xa0, 0, 0, 3, 0, 0, 0, 0]).await?;
    timeout(
        Duration::from_secs(1),
        status.wait_for(|state| state.format == Some(0xa000_0003)),
    )
    .await??;
    server
        .write_all(&[0, 0, 0, 0, 0, 0, 0, 0, 2, 0x47, 42])
        .await?;
    let HubMessage::Data(video) = subscriber.recv().await? else {
        panic!("expected video");
    };
    assert_eq!(video.as_ref(), &[0x47, 42]);
    let lease = runtime.claim()?;
    let mut packet = [0; 18];
    for sequence in 0..2_u32 {
        lease.submit(frame(), Instant::now())?;
        timeout(Duration::from_millis(200), server.read_exact(&mut packet)).await??;
        assert_eq!(packet[0], 5);
        assert_eq!(&packet[1..5], &sequence.to_be_bytes());
        assert_eq!(&packet[5..9], &9_u32.to_be_bytes());
        assert_eq!(&packet[9..], frame());
    }
    assert!(status.borrow().sent_frames >= 1);
    server.write_all(&[0x0c, 0xa0, 0, 0, 1, 0, 0, 0, 0]).await?;
    timeout(
        Duration::from_secs(1),
        status.wait_for(|state| state.format == Some(0xa000_0001)),
    )
    .await??;
    assert!(!status.borrow().microphone_enabled);
    assert_eq!(
        lease.submit(frame(), Instant::now()),
        Err(AudioLeaseError::Stale)
    );
    let next = runtime.claim()?;
    drop(lease);
    next.submit(frame(), Instant::now())?;
    timeout(Duration::from_millis(200), server.read_exact(&mut packet)).await??;
    assert_eq!(packet[0], 5);
    assert_eq!(&packet[1..5], &[0; 4]);
    task.abort();
    assert!(task.await.is_err());
    assert!(!status.borrow().connected);
    assert!(!status.borrow().microphone_enabled);
    assert_eq!(status.borrow().format, None);
    assert_eq!(
        next.submit(frame(), Instant::now()),
        Err(AudioLeaseError::Stale)
    );
    assert_eq!(server.read(&mut packet).await?, 0);
    Ok(())
}

#[tokio::test]
async fn revoked_and_expired_frames_never_reach_writer() -> Result<(), Box<dyn Error>> {
    let runtime = AudioRuntime::default();
    let (connection, mut receiver) = runtime.connect();
    connection.observe_offer(0xa000_0003)?;
    let lease = runtime.claim()?;
    lease.submit(frame(), Instant::now())?;
    let stale = receiver.recv().await.ok_or(AudioLeaseError::Closed)?;
    drop(lease);
    let mut writer = Vec::new();
    let mut uplink = Uplink::default();
    uplink.send(&mut writer, &connection, &stale).await?;
    assert!(writer.is_empty());
    let lease = runtime.claim()?;
    lease.submit(frame(), Instant::now())?;
    let fresh = receiver.recv().await.ok_or(AudioLeaseError::Closed)?;
    uplink.send(&mut writer, &connection, &fresh).await?;
    assert_eq!(&writer[..9], &[5, 0, 0, 0, 0, 0, 0, 0, 9]);
    writer.clear();
    tokio::time::sleep(Duration::from_millis(210)).await;
    uplink.send(&mut writer, &connection, &fresh).await?;
    assert!(writer.is_empty());
    Ok(())
}

#[tokio::test]
async fn blocked_audio_write_times_out() -> Result<(), Box<dyn Error>> {
    let runtime = AudioRuntime::default();
    let (connection, mut receiver) = runtime.connect();
    connection.observe_offer(0xa000_0003)?;
    let lease = runtime.claim()?;
    lease.submit(frame(), Instant::now())?;
    let audio = receiver.recv().await.ok_or(AudioLeaseError::Closed)?;
    let (mut writer, _reader) = tokio::io::duplex(1);
    let result = timeout(
        Duration::from_millis(300),
        Uplink::default().send(&mut writer, &connection, &audio),
    )
    .await?;
    assert!(
        matches!(result, Err(LiveError::Transport(error)) if error.kind() == io::ErrorKind::TimedOut)
    );
    Ok(())
}

#[tokio::test]
async fn keepalive_bytes_match_existing_wire_contract() -> Result<(), Box<dyn Error>> {
    let mut keepalive = Keepalive::default();
    let mut writer = Vec::new();
    for _ in 0..9 {
        keepalive.send(&mut writer).await?;
    }
    assert_eq!(writer, LATENCY_PACKET.repeat(9));
    writer.clear();
    keepalive.send(&mut writer).await?;
    assert_eq!(&writer[..9], &[0x0a, 0, 0, 0, 1, 0, 0, 0, 0]);
    assert_eq!(&writer[9..], &LATENCY_PACKET);
    let (mut blocked, _reader) = tokio::io::duplex(1);
    let result = timeout(
        Duration::from_millis(200),
        bounded_write(&mut blocked, &LATENCY_PACKET, Duration::from_millis(20)),
    )
    .await?;
    assert!(
        matches!(result, Err(LiveError::Transport(error)) if error.kind() == io::ErrorKind::TimedOut)
    );
    Ok(())
}
