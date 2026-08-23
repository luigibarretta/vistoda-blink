use bytes::{Buf, Bytes, BytesMut};
use thiserror::Error;

pub const MAX_PACKET_BYTES: usize = 4 * 1024 * 1024;
const HEADER_BYTES: usize = 9;
const VIDEO_MESSAGE: u8 = 0x00;
const MPEG_TS_SYNC: u8 = 0x47;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FramingError {
    #[error("payload exceeds {MAX_PACKET_BYTES} bytes")]
    Oversized,
    #[error("stream ended within an IMMI frame")]
    Truncated,
}

#[derive(Default)]
pub struct ImmiDecoder {
    buffer: BytesMut,
    pending: Option<(u8, usize)>,
}

impl ImmiDecoder {
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<Bytes>, FramingError> {
        self.buffer.extend_from_slice(chunk);
        let mut frames = Vec::new();
        loop {
            if self.pending.is_none() {
                if self.buffer.len() < HEADER_BYTES {
                    break;
                }
                let message_type = self.buffer[0];
                let payload_length = u32::from_be_bytes([
                    self.buffer[5],
                    self.buffer[6],
                    self.buffer[7],
                    self.buffer[8],
                ]) as usize;
                self.buffer.advance(HEADER_BYTES);
                if payload_length > MAX_PACKET_BYTES {
                    return Err(FramingError::Oversized);
                }
                if payload_length == 0 {
                    continue;
                }
                self.pending = Some((message_type, payload_length));
            }
            let Some((message_type, payload_length)) = self.pending else {
                continue;
            };
            if self.buffer.len() < payload_length {
                break;
            }
            let payload = self.buffer.split_to(payload_length).freeze();
            self.pending = None;
            if message_type == VIDEO_MESSAGE && payload.first() == Some(&MPEG_TS_SYNC) {
                frames.push(payload);
            }
        }
        Ok(frames)
    }

    pub fn finish(self) -> Result<(), FramingError> {
        if self.pending.is_some() || !self.buffer.is_empty() {
            Err(FramingError::Truncated)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use bytes::Bytes;

    use super::{FramingError, ImmiDecoder, MAX_PACKET_BYTES};

    fn frame(message_type: u8, payload: &[u8]) -> Vec<u8> {
        let mut value = vec![message_type, 0, 0, 0, 1];
        value.extend_from_slice(
            &u32::try_from(payload.len())
                .unwrap_or(u32::MAX)
                .to_be_bytes(),
        );
        value.extend_from_slice(payload);
        value
    }

    #[test]
    fn reassembles_fragmented_video_and_filters_control_frames() {
        let expected = [0x47, 1, 2, 3];
        let mut wire = frame(0x12, &[1, 2]);
        wire.extend(frame(0x00, &expected));
        let mut decoder = ImmiDecoder::default();
        let mut output = Vec::new();
        for chunk in wire.chunks(2) {
            output.extend(decoder.push(chunk).expect("fixture must decode"));
        }
        decoder.finish().expect("fixture must be complete");
        assert_eq!(output, vec![Bytes::copy_from_slice(&expected)]);
    }

    #[test]
    fn rejects_oversized_and_truncated_frames() {
        let mut decoder = ImmiDecoder::default();
        let mut header = vec![0, 0, 0, 0, 0];
        header.extend_from_slice(
            &u32::try_from(MAX_PACKET_BYTES + 1)
                .unwrap_or(u32::MAX)
                .to_be_bytes(),
        );
        assert_eq!(decoder.push(&header), Err(FramingError::Oversized));

        let mut decoder = ImmiDecoder::default();
        decoder
            .push(&frame(0, &[0x47, 1])[..10])
            .expect("prefix is valid");
        assert_eq!(decoder.finish(), Err(FramingError::Truncated));
    }
}
