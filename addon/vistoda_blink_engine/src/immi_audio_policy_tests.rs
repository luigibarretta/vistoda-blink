use super::*;

#[test]
fn public_eligibility_matches_policy_without_claiming_remote_ownership()
-> Result<(), AudioLeaseError> {
    for mode in [None, Some(false), Some(true)] {
        let runtime = AudioRuntime::default();
        let (connection, _frames) = runtime.connect_with_policy(mode);
        connection.observe_offer(0xa000_0001)?;
        for available in [false, true, false] {
            connection.observe_availability(available)?;
            let status = runtime.subscribe().borrow().clone();
            let supported = mode == Some(false) || (mode == Some(true) && available);
            assert_eq!(status.supported(), supported);
            assert_eq!(status.multi_client, mode);
            assert_eq!(status.audio_available, Some(available));
            assert!(!status.microphone_enabled);
            let claim = runtime.claim();
            assert_eq!(claim.is_ok(), supported);
            assert_eq!(runtime.subscribe().borrow().supported(), supported);
            drop(claim);
        }
    }
    Ok(())
}

#[tokio::test]
async fn mclv_offer_change_and_reconnect_clear_availability_and_queue_authority()
-> Result<(), AudioLeaseError> {
    let runtime = AudioRuntime::default();
    let (old, mut frames) = runtime.connect_with_policy(Some(true));
    old.observe_availability(true)?;
    old.observe_offer(0xa000_0001)?;
    let lease = runtime.claim()?;
    old.observe_offer(0xa000_0001)?;
    assert!(runtime.subscribe().borrow().microphone_enabled);
    lease.submit(
        vec![0xff, 0xf1, 0x60, 0x40, 1, 0x20, 0, 0x12, 0x34],
        Instant::now(),
    )?;
    old.observe_offer(0xa000_0003)?;
    let queued = frames.recv().await.ok_or(AudioLeaseError::Closed)?;
    assert!(!old.valid_frame(&queued, Instant::now()));
    assert_eq!(runtime.subscribe().borrow().audio_available, None);
    assert!(!runtime.subscribe().borrow().supported());
    let (current, _frames) = runtime.connect_with_policy(Some(true));
    assert_eq!(old.observe_availability(true), Err(AudioLeaseError::Closed));
    assert_eq!(old.control(), None);
    current.observe_offer(0xa000_0001)?;
    assert!(matches!(runtime.claim(), Err(AudioLeaseError::Unavailable)));
    drop(lease);
    drop(old);
    current.observe_availability(true)?;
    let _lease = runtime.claim()?;
    assert!(runtime.subscribe().borrow().microphone_enabled);
    drop(current);
    assert_eq!(runtime.subscribe().borrow().multi_client, None);
    assert_eq!(runtime.subscribe().borrow().audio_available, None);
    Ok(())
}
