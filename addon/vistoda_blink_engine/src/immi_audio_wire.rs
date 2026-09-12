//! Bounded AAC framing only; callers must separately gate camera/session ownership.
//! ADTS headers cannot prove that the enclosed AAC bitstream is decodable.

const ADTS_HEADER_BYTES: usize = 7;
pub const MAX_AUDIO_FRAME_BYTES: usize = 8191;
pub const MAX_AUDIO_CHUNK_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioWireError {
    UnsupportedFormat,
    InvalidHeader,
    InvalidLength,
    ChunkTooLarge,
    Truncated,
    Poisoned,
}

impl std::fmt::Display for AudioWireError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid Walnut audio framing: {self:?}")
    }
}
impl std::error::Error for AudioWireError {}

fn frame_length(header: &[u8]) -> Result<usize, AudioWireError> {
    // MPEG-4, no CRC, AAC-LC, 16 kHz, mono, no private/copyright flags,
    // one raw data block. Buffer fullness may vary between compliant encoders.
    if header.len() < ADTS_HEADER_BYTES
        || header[0] != 0xff
        || header[1] != 0xf1
        || header[2] != 0x60
        || header[3] & 0xfc != 0x40
        || header[6] & 3 != 0
    {
        return Err(AudioWireError::InvalidHeader);
    }
    let length = (usize::from(header[3] & 3) << 11)
        | (usize::from(header[4]) << 3)
        | usize::from(header[5] >> 5);
    if !(ADTS_HEADER_BYTES + 1..=MAX_AUDIO_FRAME_BYTES).contains(&length) {
        return Err(AudioWireError::InvalidLength);
    }
    Ok(length)
}

/// Incremental ADTS splitter. A malformed chunk permanently invalidates this decoder.
#[derive(Default)]
pub struct AdtsDecoder {
    buffer: Vec<u8>,
    expected: Option<usize>,
    failed: bool,
}

impl AdtsDecoder {
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<Vec<u8>>, AudioWireError> {
        if self.failed {
            return Err(AudioWireError::Poisoned);
        }
        let result = self.consume(chunk);
        if result.is_err() {
            self.failed = true;
            self.buffer.clear();
        }
        result
    }

    fn consume(&mut self, mut chunk: &[u8]) -> Result<Vec<Vec<u8>>, AudioWireError> {
        if chunk.len() > MAX_AUDIO_CHUNK_BYTES {
            return Err(AudioWireError::ChunkTooLarge);
        }
        let mut frames = Vec::new();
        while !chunk.is_empty() {
            let target = self.expected.unwrap_or(ADTS_HEADER_BYTES);
            let count = (target - self.buffer.len()).min(chunk.len());
            self.buffer.extend_from_slice(&chunk[..count]);
            chunk = &chunk[count..];
            if self.buffer.len() < target {
                continue;
            }
            if self.expected.is_none() {
                self.expected = Some(frame_length(&self.buffer)?);
            } else {
                frames.push(std::mem::take(&mut self.buffer));
                self.expected = None;
            }
        }
        Ok(frames)
    }

    pub fn finish(self) -> Result<(), AudioWireError> {
        if self.failed {
            Err(AudioWireError::Poisoned)
        } else if self.buffer.is_empty() {
            Ok(())
        } else {
            Err(AudioWireError::Truncated)
        }
    }
}

/// Serialize an AAC format. No native caller of sendAudioConfig is verified;
/// never emit this automatically on receiving an offer or enabling a microphone.
pub fn audio_config(format: u32) -> Result<[u8; 9], AudioWireError> {
    if !matches!(format, 0xa000_0001 | 0xa000_0003) {
        return Err(AudioWireError::UnsupportedFormat);
    }
    let mut header = [0_u8; 9];
    header[0] = 0x0c;
    header[1..5].copy_from_slice(&format.to_be_bytes());
    Ok(header)
}

/// Fresh writer per microphone activation: native enable resets the counter.
/// Start at zero, wrap at 32 bits, and discard the writer on mute/disconnect.
#[derive(Default, Clone, Copy)]
pub struct AudioPacketWriter {
    sequence: u32,
}

impl AudioPacketWriter {
    pub fn audio_frame(&mut self, frame: &[u8]) -> Result<Vec<u8>, AudioWireError> {
        let length = frame_length(frame)?;
        if frame.len() != length {
            return Err(AudioWireError::InvalidLength);
        }
        let length = u32::try_from(length).map_err(|_| AudioWireError::InvalidLength)?;
        let mut packet = Vec::with_capacity(9 + frame.len());
        packet.push(0x05);
        packet.extend_from_slice(&self.sequence.to_be_bytes());
        packet.extend_from_slice(&length.to_be_bytes());
        packet.extend_from_slice(frame);
        // Native Walnut ADTS uses fullness=0; preserve length and AAC payload.
        packet[9 + 5] &= 0xe0;
        packet[9 + 6] &= 0x03;
        self.sequence = self.sequence.wrapping_add(1);
        Ok(packet)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Structurally valid header and synthetic payload, not an audible AAC fixture.
    const FRAME: [u8; 9] = [0xff, 0xf1, 0x60, 0x40, 1, 0x20, 0, 0x12, 0x34];

    #[test]
    fn fragments_and_consecutive_frames_preserve_payload() -> Result<(), AudioWireError> {
        let stream = FRAME.repeat(3);
        for boundary in 0..=stream.len() {
            let mut decoder = AdtsDecoder::default();
            let mut frames = decoder.push(&stream[..boundary])?;
            frames.extend(decoder.push(&stream[boundary..])?);
            assert_eq!(frames, vec![FRAME.to_vec(); 3]);
            decoder.finish()?;
        }
        let mut decoder = AdtsDecoder::default();
        let mut frames = Vec::new();
        for byte in stream {
            frames.extend(decoder.push(&[byte])?);
        }
        assert_eq!(frames.len(), 3);
        decoder.finish()
    }

    #[test]
    fn rejects_codec_header_variants_and_poisoned_reuse() {
        for (offset, value) in [
            (0, 0),
            (1, 0xf9),
            (1, 0xf0),
            (2, 0x20),
            (2, 0x64),
            (2, 0x62),
            (3, 0x80),
            (3, 0x44),
            (6, 1),
        ] {
            let mut frame = FRAME;
            frame[offset] = value;
            let mut decoder = AdtsDecoder::default();
            assert_eq!(decoder.push(&frame), Err(AudioWireError::InvalidHeader));
            assert_eq!(decoder.push(&FRAME), Err(AudioWireError::Poisoned));
            assert_eq!(decoder.finish(), Err(AudioWireError::Poisoned));
        }
    }

    #[test]
    fn lengths_and_input_memory_are_bounded() -> Result<(), AudioWireError> {
        for length in 0..=7_u8 {
            let mut frame = FRAME;
            frame[4] = 0;
            frame[5] = length << 5;
            assert_eq!(
                AdtsDecoder::default().push(&frame),
                Err(AudioWireError::InvalidLength)
            );
        }
        let mut frame = vec![0_u8; MAX_AUDIO_FRAME_BYTES];
        frame[..7].copy_from_slice(&[0xff, 0xf1, 0x60, 0x43, 0xff, 0xff, 0xfc]);
        let mut decoder = AdtsDecoder::default();
        assert_eq!(decoder.push(&frame)?, vec![frame]);
        decoder.finish()?;
        assert_eq!(
            AdtsDecoder::default().push(&vec![0; MAX_AUDIO_CHUNK_BYTES + 1]),
            Err(AudioWireError::ChunkTooLarge)
        );
        Ok(())
    }

    #[test]
    fn rejects_every_truncated_prefix() -> Result<(), AudioWireError> {
        for length in 1..FRAME.len() {
            let mut decoder = AdtsDecoder::default();
            assert!(decoder.push(&FRAME[..length])?.is_empty());
            assert_eq!(decoder.finish(), Err(AudioWireError::Truncated));
        }
        AdtsDecoder::default().finish()
    }

    #[test]
    fn exact_immi_wire_and_counter_wrap() -> Result<(), AudioWireError> {
        assert_eq!(
            audio_config(0xa000_0001)?,
            [0x0c, 0xa0, 0, 0, 1, 0, 0, 0, 0]
        );
        assert_eq!(
            audio_config(0xa000_0003)?,
            [0x0c, 0xa0, 0, 0, 3, 0, 0, 0, 0]
        );
        for format in [0, 0xa000_0000, 0xa000_0002, 0xa000_0004, u32::MAX] {
            assert_eq!(audio_config(format), Err(AudioWireError::UnsupportedFormat));
        }
        let mut writer = AudioPacketWriter::default();
        assert!(writer.audio_frame(&FRAME[..8]).is_err());
        let packet = writer.audio_frame(&FRAME)?;
        assert_eq!(&packet[..9], &[5, 0, 0, 0, 0, 0, 0, 0, 9]);
        assert_eq!(&packet[9..], &FRAME);
        assert_eq!(&writer.audio_frame(&FRAME)?[1..5], &1_u32.to_be_bytes());
        assert!(writer.audio_frame(&FRAME.repeat(2)).is_err());
        writer.sequence = u32::MAX;
        assert_eq!(&writer.audio_frame(&FRAME)?[1..5], &u32::MAX.to_be_bytes());
        assert_eq!(&writer.audio_frame(&FRAME)?[1..5], &0_u32.to_be_bytes());
        Ok(())
    }
}
