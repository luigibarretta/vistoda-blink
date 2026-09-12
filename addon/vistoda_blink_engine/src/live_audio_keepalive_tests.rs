use super::*;
use crate::{
    hub::{CameraHub, HubMessage},
    immi_audio_lease::{AudioLeaseError, AudioRuntime, AudioStatus},
};
use std::{error::Error, sync::Arc, time::Instant};
use tokio::time::timeout;

#[tokio::test]
async fn keepalive_budget_tracks_desire_and_pending_stop_not_old_packet_epoch()
-> Result<(), Box<dyn Error>> {
    let runtime = AudioRuntime::default();
    let (connection, mut frames) = runtime.connect_with_policy(Some(true));
    connection.observe_offer(0xa000_0001)?;
    connection.observe_availability(true)?;
    let mut uplink = Uplink::default();
    let mut writer = Vec::new();
    assert_eq!(uplink.keepalive_budget(&connection), Duration::from_secs(5));
    let lease = runtime.claim()?;
    assert_eq!(
        uplink.keepalive_budget(&connection),
        Duration::from_millis(200)
    );
    uplink.reconcile(&mut writer, &connection).await?;
    lease.submit(
        vec![0xff, 0xf1, 0x60, 0x40, 1, 0x20, 0, 0x12, 0x34],
        Instant::now(),
    )?;
    let audio = frames.recv().await.ok_or(AudioLeaseError::Closed)?;
    uplink.send(&mut writer, &connection, &audio).await?;
    assert_eq!(writer.len(), 27);
    writer.clear();
    drop(lease);
    assert!(!runtime.subscribe().borrow().microphone_enabled);
    assert_eq!(
        uplink.keepalive_budget(&connection),
        Duration::from_millis(200)
    );
    uplink.reconcile(&mut writer, &connection).await?;
    assert_eq!(writer, [0x17, 0, 0, 0, 4, 0, 0, 0, 0]);
    assert_eq!(uplink.keepalive_budget(&connection), Duration::from_secs(5));
    Ok(())
}

#[tokio::test]
async fn passive_backpressure_keeps_video_but_active_microphone_times_out()
-> Result<(), Box<dyn Error>> {
    for active in [false, true] {
        let hub = Arc::new(CameraHub::new());
        let publisher = hub.acquire_publisher(1)?;
        let mut subscriber = hub.subscribe();
        let runtime = subscriber.audio();
        let mut status = runtime.subscribe();
        let (client, mut server) = tokio::io::duplex(1);
        let task =
            tokio::spawn(async move { receive_stream(client, &publisher, Some(true)).await });
        server
            .write_all(&[
                0x0c, 0xa0, 0, 0, 1, 0, 0, 0, 0, 0x18, 0, 0, 0, 4, 0, 0, 0, 0,
            ])
            .await?;
        timeout(
            Duration::from_secs(1),
            status.wait_for(AudioStatus::supported),
        )
        .await??;
        let _lease = if active {
            let lease = runtime.claim()?;
            let mut start = [0; 9];
            timeout(Duration::from_millis(500), server.read_exact(&mut start)).await??;
            assert_eq!(start, [0x17, 0, 0, 0, 3, 0, 0, 0, 0]);
            Some(lease)
        } else {
            None
        };
        let mut prefix = [0; 1];
        timeout(Duration::from_secs(2), server.read_exact(&mut prefix)).await??;
        assert_eq!(prefix, [0x12]);
        tokio::time::sleep(Duration::from_millis(300)).await;
        if active {
            assert!(timeout(Duration::from_millis(300), task).await??.is_err());
            assert!(!status.borrow().connected);
            assert!(!status.borrow().microphone_enabled);
        } else {
            assert!(!task.is_finished());
            let mut rest = [0; 32];
            timeout(Duration::from_millis(500), server.read_exact(&mut rest)).await??;
            assert_eq!(&rest, &LATENCY_PACKET[1..]);
            server
                .write_all(&[0, 0, 0, 0, 0, 0, 0, 0, 2, 0x47, 42])
                .await?;
            assert!(
                matches!(timeout(Duration::from_millis(500), subscriber.recv()).await??,
                HubMessage::Data(bytes) if bytes.as_ref() == [0x47, 42])
            );
            assert!(status.borrow().connected);
            task.abort();
            assert!(task.await.is_err());
        }
    }
    Ok(())
}
