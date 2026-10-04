use bytes::Bytes;
use futures_util::stream;

use super::{ARGUMENTS, MAX_JPEG_BYTES, extract, is_jpeg};
use crate::motion::MotionTracker;

/// A fake decoder that consumes two TS packets and prints a minimal JPEG.
const FAKE_DECODER: &str = r"n=$(head -c 376 | wc -c); [ $n -eq 376 ] || exit 1
printf '\377\330\377'; head -c 200 /dev/zero; printf '\377\331'";

fn packets(count: usize, first: u8) -> impl futures_util::Stream<Item = Bytes> {
    let mut packet = vec![0_u8; 188];
    packet[0] = first;
    stream::iter(vec![Bytes::from(packet); count])
}

#[test]
fn decoder_arguments_are_pipe_only_and_single_frame() {
    assert_eq!(
        ARGUMENTS.iter().filter(|item| **item == "pipe:0").count(),
        1
    );
    assert_eq!(
        ARGUMENTS.iter().filter(|item| **item == "pipe:1").count(),
        1
    );
    assert!(!ARGUMENTS.iter().any(|item| item.contains("://")));
    assert!(!ARGUMENTS.contains(&"-y"), "the decoder never writes files");
    let frames = ARGUMENTS.iter().position(|item| *item == "-frames:v");
    assert_eq!(frames.map(|index| ARGUMENTS[index + 1]), Some("1"));
    assert!(ARGUMENTS.contains(&"nokey"), "only keyframes are decoded");
}

#[test]
fn only_bounded_complete_jpegs_are_accepted() {
    let mut jpeg = vec![0xFF, 0xD8, 0xFF];
    jpeg.extend([0_u8; 200]);
    jpeg.extend([0xFF, 0xD9]);
    assert!(is_jpeg(&jpeg));
    assert!(
        !is_jpeg(&jpeg[..jpeg.len() - 1]),
        "a truncated JPEG is rejected"
    );
    assert!(!is_jpeg(&[0xFF, 0xD8, 0xFF, 0xFF, 0xD9]), "too small");
    let mut huge = vec![0xFF, 0xD8, 0xFF];
    huge.resize(MAX_JPEG_BYTES, 0);
    huge.extend([0xFF, 0xD9]);
    assert!(!is_jpeg(&huge), "larger than the cap");
}

#[tokio::test]
async fn extracts_one_jpeg_from_a_live_like_stream() {
    let jpeg = extract("/bin/sh", &["-c", FAKE_DECODER], packets(64, 0x47)).await;
    assert!(jpeg.is_some_and(|jpeg| is_jpeg(&jpeg)));
}

#[tokio::test]
async fn non_ts_input_or_invalid_output_yield_no_still() {
    // A non-TS first chunk ends the input before anything reaches the decoder.
    let none = extract("/bin/sh", &["-c", FAKE_DECODER], packets(64, 0x00)).await;
    assert!(none.is_none());
    let text = extract(
        "/bin/sh",
        &["-c", "cat >/dev/null; echo no"],
        packets(4, 0x47),
    )
    .await;
    assert!(text.is_none());
    let missing = extract("/nonexistent/ffmpeg", ARGUMENTS, packets(4, 0x47)).await;
    assert!(missing.is_none());
}

#[tokio::test]
async fn stills_are_bounded_and_signal_a_motion_change() {
    let tracker = MotionTracker::default();
    let before = tracker.sequence();
    for index in 0..20 {
        tracker
            .store_still(&index.to_string(), Bytes::from_static(b"jpeg"))
            .await;
    }
    assert!(tracker.sequence() > before);
    assert!(tracker.local_still("0").await.is_none(), "oldest evicted");
    assert!(tracker.local_still("19").await.is_some());
    let guard = tracker.stills.inner.read().await;
    assert_eq!(guard.0.len(), 16);
    assert_eq!(guard.1.len(), 16);
}
