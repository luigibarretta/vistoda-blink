//! Offline protocol observations, not proof of camera talk support.
#![allow(clippy::unwrap_used)]
use vistoda_blink_engine::framing::{FramingError, ImmiDecoder, ImmiEvent};

fn offer(value: u32) -> Vec<u8> {
    let mut bytes = vec![0x0c];
    bytes.extend(value.to_be_bytes());
    bytes.extend([0; 4]);
    bytes
}

#[test]
fn header_only_audio_offer_survives_every_fragment_boundary() {
    let wire = offer(0xa000_0002);
    for boundary in 0..=wire.len() {
        let mut decoder = ImmiDecoder::default();
        let mut events = decoder.push_events(&wire[..boundary]).unwrap();
        events.extend(decoder.push_events(&wire[boundary..]).unwrap());
        assert_eq!(events, vec![ImmiEvent::AudioConfig(0xa000_0002)]);
        decoder.finish().unwrap();
    }
}

#[test]
fn existing_video_api_does_not_expose_control_bytes() {
    let mut wire = offer(0xa000_0002);
    wire.extend([0, 0, 0, 0, 1, 0, 0, 0, 2, 0x47, 1]);
    wire.extend(offer(0xffff_ffff));
    let mut decoder = ImmiDecoder::default();
    let frames = decoder.push(&wire).unwrap();
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].as_ref(), &[0x47, 1]);
    decoder.finish().unwrap();
}

#[test]
fn unknown_formats_are_observed_without_claiming_support() {
    let mut decoder = ImmiDecoder::default();
    assert_eq!(
        decoder.push_events(&offer(0xffff_ffff)).unwrap(),
        vec![ImmiEvent::AudioConfig(0xffff_ffff)]
    );
}

#[test]
fn malformed_offer_payload_is_not_promoted_to_audio_configuration() {
    let mut wire = offer(0xa000_0002);
    wire[8] = 1;
    wire.push(0x47);
    let mut decoder = ImmiDecoder::default();
    assert!(decoder.push_events(&wire).unwrap().is_empty());
    decoder.finish().unwrap();
}

#[test]
fn partial_audio_header_is_truncated() {
    let mut decoder = ImmiDecoder::default();
    assert!(decoder.push_events(&offer(0)[..8]).unwrap().is_empty());
    assert_eq!(decoder.finish(), Err(FramingError::Truncated));
}
