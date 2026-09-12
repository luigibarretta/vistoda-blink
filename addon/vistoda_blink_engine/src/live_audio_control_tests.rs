use super::*;
use crate::immi_audio_lease::{AudioLeaseError, AudioRuntime};
use std::{
    error::Error,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;

fn frame() -> Vec<u8> {
    vec![0xff, 0xf1, 0x60, 0x40, 1, 0x20, 0, 0x12, 0x34]
}

#[tokio::test]
async fn policy_and_availability_do_not_imply_ownership() -> Result<(), Box<dyn Error>> {
    for policy in [None, Some(false), Some(true)] {
        let runtime = AudioRuntime::default();
        let (connection, _frames) = runtime.connect_with_policy(policy);
        connection.observe_offer(0xa000_0001)?;
        let mut uplink = Uplink::default();
        let mut bytes = Vec::new();
        uplink.reconcile(&mut bytes, &connection).await?;
        assert!(bytes.is_empty());
        let claim = runtime.claim();
        assert_eq!(claim.is_ok(), policy == Some(false));
        drop(claim);
        connection.observe_availability(true)?;
        uplink.reconcile(&mut bytes, &connection).await?;
        assert!(bytes.is_empty());
        let claim = runtime.claim();
        assert_eq!(claim.is_ok(), policy.is_some());
        uplink.reconcile(&mut bytes, &connection).await?;
        assert_eq!(
            bytes,
            if policy == Some(true) {
                START.to_vec()
            } else {
                vec![]
            }
        );
    }
    Ok(())
}

#[tokio::test]
async fn mclv_orders_start_frames_stop_and_fresh_epoch() -> Result<(), Box<dyn Error>> {
    let runtime = AudioRuntime::default();
    let (connection, mut frames) = runtime.connect_with_policy(Some(true));
    connection.observe_offer(0xa000_0001)?;
    connection.observe_availability(true)?;
    let mut uplink = Uplink::default();
    let mut bytes = Vec::new();
    let lease = runtime.claim()?;
    for counter in 0..2_u32 {
        lease.submit(frame(), Instant::now())?;
        let audio = frames.recv().await.ok_or(AudioLeaseError::Closed)?;
        uplink.send(&mut bytes, &connection, &audio).await?;
        let offset = 9 + usize::try_from(counter)? * 18;
        assert_eq!(
            &bytes[offset..offset + 9],
            &[5, 0, 0, 0, u8::try_from(counter)?, 0, 0, 0, 9]
        );
    }
    assert_eq!(&bytes[..9], &START);
    assert!(matches!(runtime.claim(), Err(AudioLeaseError::Busy)));
    drop(lease);
    let next = runtime.claim()?; // Coalesced drop + claim must still stop the old epoch.
    bytes.clear();
    uplink.reconcile(&mut bytes, &connection).await?;
    assert_eq!(bytes, [STOP, START].concat());
    bytes.clear();
    next.submit(frame(), Instant::now())?;
    uplink
        .send(
            &mut bytes,
            &connection,
            &frames.recv().await.ok_or(AudioLeaseError::Closed)?,
        )
        .await?;
    assert_eq!(&bytes[..9], &[5, 0, 0, 0, 0, 0, 0, 0, 9]);
    drop(next);
    bytes.clear();
    uplink.reconcile(&mut bytes, &connection).await?;
    uplink.reconcile(&mut bytes, &connection).await?;
    assert_eq!(bytes, STOP);
    bytes.clear();
    drop(runtime.claim()?); // A cancelled desire that never started needs no Stop.
    uplink.reconcile(&mut bytes, &connection).await?;
    assert!(bytes.is_empty());
    Ok(())
}

#[tokio::test]
async fn audio_wait_revokes_without_automatic_reclaim() -> Result<(), Box<dyn Error>> {
    let runtime = AudioRuntime::default();
    let (connection, mut frames) = runtime.connect_with_policy(Some(true));
    connection.observe_offer(0xa000_0001)?;
    connection.observe_availability(true)?;
    let lease = runtime.claim()?;
    let mut changes = connection.changes();
    let mut uplink = Uplink::default();
    let mut bytes = Vec::new();
    uplink.reconcile(&mut bytes, &connection).await?;
    lease.submit(frame(), Instant::now())?;
    let queued = frames.recv().await.ok_or(AudioLeaseError::Closed)?;
    connection.observe_availability(false)?;
    assert!(changes.has_changed()?);
    changes.borrow_and_update();
    assert!(!runtime.subscribe().borrow().microphone_enabled);
    assert!(matches!(runtime.claim(), Err(AudioLeaseError::Unavailable)));
    bytes.clear();
    uplink.send(&mut bytes, &connection, &queued).await?;
    assert_eq!(bytes, STOP);
    bytes.clear();
    connection.observe_availability(true)?;
    uplink.reconcile(&mut bytes, &connection).await?;
    assert!(bytes.is_empty());
    assert_eq!(
        lease.submit(frame(), Instant::now()),
        Err(AudioLeaseError::Stale)
    );
    Ok(())
}

/// Revoke during the first write, after the caller's authorization check.
struct RevokeWriter {
    lease: Option<crate::immi_audio_lease::AudioLease>,
    bytes: Vec<u8>,
}
impl AsyncWrite for RevokeWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.lease.take();
        self.bytes.extend_from_slice(bytes);
        Poll::Ready(Ok(bytes.len()))
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[tokio::test]
async fn revocation_during_start_pairs_stop_and_never_sends_stale_audio()
-> Result<(), Box<dyn Error>> {
    let runtime = AudioRuntime::default();
    let (connection, mut frames) = runtime.connect_with_policy(Some(true));
    connection.observe_offer(0xa000_0001)?;
    connection.observe_availability(true)?;
    let lease = runtime.claim()?;
    lease.submit(frame(), Instant::now())?;
    let queued = frames.recv().await.ok_or(AudioLeaseError::Closed)?;
    let mut writer = RevokeWriter {
        lease: Some(lease),
        bytes: Vec::new(),
    };
    Uplink::default()
        .send(&mut writer, &connection, &queued)
        .await?;
    assert_eq!(writer.bytes, [START, STOP].concat());
    assert_eq!(runtime.subscribe().borrow().sent_frames, 0);
    writer.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn partial_control_timeout_fails_closed() -> Result<(), Box<dyn Error>> {
    let runtime = AudioRuntime::default();
    let (connection, _frames) = runtime.connect_with_policy(Some(true));
    connection.observe_offer(0xa000_0001)?;
    connection.observe_availability(true)?;
    let _lease = runtime.claim()?;
    let (mut writer, _reader) = tokio::io::duplex(1);
    let result = Uplink::default().reconcile(&mut writer, &connection).await;
    assert!(
        matches!(result, Err(LiveError::Transport(error)) if error.kind() == io::ErrorKind::TimedOut)
    );
    Ok(())
}

#[tokio::test]
async fn delayed_start_cannot_refresh_queued_pcm_age() -> Result<(), Box<dyn Error>> {
    let runtime = AudioRuntime::default();
    let (connection, mut frames) = runtime.connect_with_policy(Some(true));
    connection.observe_offer(0xa000_0001)?;
    connection.observe_availability(true)?;
    let lease = runtime.claim()?;
    let created = Instant::now()
        .checked_sub(Duration::from_millis(180))
        .ok_or(AudioLeaseError::Stale)?;
    lease.submit(frame(), created)?;
    let queued = frames.recv().await.ok_or(AudioLeaseError::Closed)?;
    let (mut writer, mut reader) = tokio::io::duplex(1);
    let task = tokio::spawn(async move {
        Uplink::default()
            .send(&mut writer, &connection, &queued)
            .await
    });
    tokio::time::sleep(Duration::from_millis(40)).await;
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).await?;
    task.await??;
    assert_eq!(bytes, START);
    assert_eq!(runtime.subscribe().borrow().sent_frames, 0);
    Ok(())
}

#[tokio::test]
async fn prepared_but_expired_packet_does_not_consume_counter() -> Result<(), Box<dyn Error>> {
    let runtime = AudioRuntime::default();
    let (connection, mut frames) = runtime.connect();
    connection.observe_offer(0xa000_0001)?;
    let lease = runtime.claim()?;
    lease.submit(frame(), Instant::now())?;
    let queued = frames.recv().await.ok_or(AudioLeaseError::Closed)?;
    let mut uplink = Uplink::default();
    let mut candidate = uplink.packets;
    let packet = candidate.audio_frame(queued.payload())?;
    tokio::time::sleep(Duration::from_millis(210)).await;
    let mut bytes = Vec::new();
    uplink
        .write_frame(&mut bytes, &connection, &queued, candidate, &packet)
        .await?;
    assert!(bytes.is_empty());
    lease.submit(frame(), Instant::now())?;
    let fresh = frames.recv().await.ok_or(AudioLeaseError::Closed)?;
    uplink.send(&mut bytes, &connection, &fresh).await?;
    assert_eq!(&bytes[..9], &[5, 0, 0, 0, 0, 0, 0, 0, 9]);
    Ok(())
}
