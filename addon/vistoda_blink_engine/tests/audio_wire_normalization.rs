use vistoda_blink_engine::immi_audio_wire::AudioPacketWriter;

#[test]
fn native_fullness_preserves_every_frame_length_and_payload()
-> Result<(), Box<dyn std::error::Error>> {
    for length in 8..=8191_usize {
        let mut frame = vec![0x5a; length];
        frame[..7].copy_from_slice(&[
            0xff,
            0xf1,
            0x60,
            0x40 | u8::try_from(length >> 11)?,
            u8::try_from((length >> 3) & 0xff)?,
            u8::try_from(length & 7)? << 5 | 0x1f,
            0xfc,
        ]);
        let packet = AudioPacketWriter::default().audio_frame(&frame)?;
        assert_eq!(&packet[9..14], &frame[..5]);
        assert_eq!(packet[14], frame[5] & 0xe0);
        assert_eq!(packet[15], 0);
        assert_eq!(&packet[16..], &frame[7..]);
        assert_eq!(packet.len(), 9 + length);
        assert_eq!(&packet[5..9], &u32::try_from(length)?.to_be_bytes());
    }
    Ok(())
}
