use super::*;

fn frame() -> Vec<u8> {
    vec![0xff, 0xf1, 0x60, 0x40, 1, 0x20, 0, 0x12, 0x34]
}

#[tokio::test]
async fn offer_and_exclusive_owner_gate_capture() -> Result<(), AudioLeaseError> {
    let runtime = AudioRuntime::default();
    let status = runtime.subscribe();
    assert!(matches!(runtime.claim(), Err(AudioLeaseError::Closed)));
    let (connection, mut receiver) = runtime.connect();
    assert!(status.borrow().connected);
    for offer in [0, 0xa000_0000, 0xa000_0002, 0xa000_0006, u32::MAX] {
        connection.observe_offer(offer)?;
        assert!(matches!(
            runtime.claim(),
            Err(AudioLeaseError::UnsupportedOffer)
        ));
        assert!(!status.borrow().microphone_enabled);
    }
    connection.observe_offer(0xa000_0003)?;
    let lease = runtime.claim()?;
    assert!(status.borrow().microphone_enabled);
    assert!(matches!(runtime.claim(), Err(AudioLeaseError::Busy)));
    let created = Instant::now();
    lease.submit(frame(), created)?;
    let audio = receiver.recv().await.ok_or(AudioLeaseError::Closed)?;
    assert_eq!(audio.payload(), frame());
    assert_eq!(audio.remaining(created), Some(MAX_AUDIO_AGE));
    assert_eq!(
        audio.remaining(created + MAX_AUDIO_AGE),
        Some(Duration::ZERO)
    );
    assert_eq!(
        audio.remaining(created + MAX_AUDIO_AGE + Duration::from_nanos(1)),
        None
    );
    assert!(connection.valid_frame(&audio, created));
    assert!(!connection.valid_frame(&audio, created + MAX_AUDIO_AGE + Duration::from_nanos(1)));
    let before = created
        .checked_sub(Duration::from_nanos(1))
        .ok_or(AudioLeaseError::Stale)?;
    assert!(!connection.valid_frame(&audio, before));
    drop(lease);
    assert!(!status.borrow().microphone_enabled);
    assert!(!connection.valid_frame(&audio, created));
    Ok(())
}

#[tokio::test]
async fn mute_and_reenable_reject_queued_audio_and_reset_counter() -> Result<(), AudioLeaseError> {
    let runtime = AudioRuntime::default();
    let (connection, mut receiver) = runtime.connect();
    connection.observe_offer(0xa000_0001)?;
    let lease = runtime.claim()?;
    lease.submit(frame(), Instant::now())?;
    drop(lease);
    let next = runtime.claim()?;
    next.submit(frame(), Instant::now())?;
    let old = receiver.recv().await.ok_or(AudioLeaseError::Closed)?;
    let new = receiver.recv().await.ok_or(AudioLeaseError::Closed)?;
    assert_ne!(old.epoch(), new.epoch());
    assert!(!connection.valid_frame(&old, Instant::now()));
    assert!(connection.valid_frame(&new, Instant::now()));
    let packet = AudioPacketWriter::default()
        .audio_frame(new.payload())
        .map_err(|_| AudioLeaseError::InvalidFrame)?;
    assert_eq!(&packet[1..5], &[0; 4]);
    Ok(())
}

#[tokio::test]
async fn reconnect_and_stale_guard_drops_cannot_revoke_new_owner() -> Result<(), AudioLeaseError> {
    let runtime = AudioRuntime::default();
    let (old_connection, mut old_receiver) = runtime.connect();
    old_connection.observe_offer(0xa000_0003)?;
    let old_lease = runtime.claim()?;
    old_lease.submit(frame(), Instant::now())?;
    let (connection, mut receiver) = runtime.connect();
    assert_eq!(runtime.subscribe().borrow().format, None);
    assert_eq!(
        old_connection.observe_offer(0xa000_0003),
        Err(AudioLeaseError::Closed)
    );
    assert_eq!(
        old_lease.submit(frame(), Instant::now()),
        Err(AudioLeaseError::Stale)
    );
    let stale = old_receiver.recv().await.ok_or(AudioLeaseError::Closed)?;
    assert!(!old_connection.valid_frame(&stale, Instant::now()));
    assert!(!connection.valid_frame(&stale, Instant::now()));
    assert!(old_receiver.recv().await.is_none());
    connection.observe_offer(0xa000_0003)?;
    let lease = runtime.claim()?;
    drop(old_connection);
    drop(old_lease);
    lease.submit(frame(), Instant::now())?;
    assert!(connection.valid_frame(
        &receiver.recv().await.ok_or(AudioLeaseError::Closed)?,
        Instant::now()
    ));
    drop(connection);
    assert_eq!(*runtime.subscribe().borrow(), AudioStatus::default());
    assert!(receiver.recv().await.is_none());
    assert_eq!(
        lease.submit(frame(), Instant::now()),
        Err(AudioLeaseError::Stale)
    );
    Ok(())
}

#[tokio::test]
async fn changed_offer_revokes_but_repeat_offer_preserves_owner() -> Result<(), AudioLeaseError> {
    let runtime = AudioRuntime::default();
    let (connection, mut receiver) = runtime.connect();
    connection.observe_offer(0xa000_0003)?;
    let lease = runtime.claim()?;
    connection.observe_offer(0xa000_0003)?;
    lease.submit(frame(), Instant::now())?;
    connection.observe_offer(0xa000_0001)?;
    let stale = receiver.recv().await.ok_or(AudioLeaseError::Closed)?;
    assert!(!connection.valid_frame(&stale, Instant::now()));
    assert_eq!(
        lease.submit(frame(), Instant::now()),
        Err(AudioLeaseError::Stale)
    );
    let next = runtime.claim()?;
    drop(lease);
    next.submit(frame(), Instant::now())?;
    connection.observe_offer(u32::MAX)?;
    assert!(!runtime.subscribe().borrow().microphone_enabled);
    assert!(matches!(
        runtime.claim(),
        Err(AudioLeaseError::UnsupportedOffer)
    ));
    Ok(())
}

#[tokio::test]
async fn malformed_stale_future_and_excess_frames_are_rejected() -> Result<(), AudioLeaseError> {
    let runtime = AudioRuntime::default();
    let (connection, mut receiver) = runtime.connect();
    connection.observe_offer(0xa000_0003)?;
    let lease = runtime.claim()?;
    assert_eq!(
        lease.submit(vec![0; 9], Instant::now()),
        Err(AudioLeaseError::InvalidFrame)
    );
    let stale = Instant::now()
        .checked_sub(Duration::from_secs(1))
        .ok_or(AudioLeaseError::Stale)?;
    assert_eq!(lease.submit(frame(), stale), Err(AudioLeaseError::Expired));
    assert_eq!(
        lease.submit(frame(), Instant::now() + Duration::from_secs(1)),
        Err(AudioLeaseError::Expired)
    );
    assert!(receiver.try_recv().is_err());
    for _ in 0..AUDIO_QUEUE_DEPTH {
        lease.submit(frame(), Instant::now())?;
    }
    assert_eq!(
        lease.submit(frame(), Instant::now()),
        Err(AudioLeaseError::Backpressure)
    );
    assert_eq!(receiver.len(), AUDIO_QUEUE_DEPTH);
    // Rejecting a new frame never revokes or displaces queued speech.
    assert!(runtime.subscribe().borrow().microphone_enabled);
    assert!(receiver.try_recv().is_ok());
    lease.submit(frame(), Instant::now())?;
    assert_eq!(receiver.len(), AUDIO_QUEUE_DEPTH);
    connection.observe_offer(0xa000_0001)?;
    // Revocation wins over expiry, so callers cannot mistake it for a safe drop.
    assert_eq!(lease.submit(frame(), stale), Err(AudioLeaseError::Stale));
    let lease = runtime.claim()?;
    drop(receiver);
    assert_eq!(
        lease.submit(frame(), Instant::now()),
        Err(AudioLeaseError::Closed)
    );
    Ok(())
}

#[tokio::test]
async fn aborted_connection_task_synchronously_revokes_lease() -> Result<(), AudioLeaseError> {
    let runtime = AudioRuntime::default();
    let (connection, receiver) = runtime.connect();
    connection.observe_offer(0xa000_0003)?;
    let lease = runtime.claim()?;
    let status = runtime.subscribe();
    let task = tokio::spawn(async move {
        let _guards = (connection, receiver);
        std::future::pending::<()>().await;
    });
    task.abort();
    assert!(task.await.is_err());
    assert_eq!(*status.borrow(), AudioStatus::default());
    assert_eq!(
        lease.submit(frame(), Instant::now()),
        Err(AudioLeaseError::Stale)
    );
    Ok(())
}
