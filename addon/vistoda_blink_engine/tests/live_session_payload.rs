use vistoda_blink_engine::framing::{FramingError, ImmiDecoder, ImmiEvent, MAX_PACKET_BYTES};

#[test]
fn payload_availability_emits_only_after_complete_bounded_frame() -> Result<(), FramingError> {
    for command in [4_u32, 5] {
        let mut wire = vec![0x18];
        wire.extend_from_slice(&command.to_be_bytes());
        wire.extend_from_slice(&3_u32.to_be_bytes());
        wire.extend_from_slice(&[0x47, 0, 0xff]);
        for split in 1..wire.len() {
            let mut decoder = ImmiDecoder::default();
            assert!(decoder.push_events(&wire[..split])?.is_empty());
            assert_eq!(
                decoder.push_events(&wire[split..])?,
                [ImmiEvent::AudioAvailability(command == 4)]
            );
            decoder.finish()?;
        }
        let mut decoder = ImmiDecoder::default();
        assert!(decoder.push_events(&wire[..wire.len() - 1])?.is_empty());
        assert_eq!(decoder.finish(), Err(FramingError::Truncated));
        assert!(ImmiDecoder::default().push(&wire)?.is_empty());
    }
    Ok(())
}

#[test]
fn payload_status_cannot_desynchronize_video_or_authorize_unknown_commands()
-> Result<(), FramingError> {
    let mut wire = vec![0x18, 0, 0, 0, 99, 0, 0, 0, 2, 0x47, 1];
    wire.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 2, 0x47, 42]);
    assert_eq!(
        ImmiDecoder::default().push_events(&wire)?,
        [ImmiEvent::Video(bytes::Bytes::from_static(&[0x47, 42]))]
    );
    let mut oversized = vec![0x18, 0, 0, 0, 4];
    oversized.extend_from_slice(
        &u32::try_from(MAX_PACKET_BYTES + 1)
            .map_err(|_| FramingError::Oversized)?
            .to_be_bytes(),
    );
    assert_eq!(
        ImmiDecoder::default().push_events(&oversized),
        Err(FramingError::Oversized)
    );
    Ok(())
}
