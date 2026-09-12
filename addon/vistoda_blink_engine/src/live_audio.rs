//! One future owns IMMI transport, audio authorization and bounded writes.
use crate::{
    framing::{ImmiDecoder, ImmiEvent},
    hub::PublisherGuard,
    immi_audio::AudioOffer,
    immi_audio_lease::{AudioConnection, AudioFrame},
    immi_audio_wire::AudioPacketWriter,
    live::LiveError,
};
use std::{
    io,
    time::{Duration, Instant},
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const KEEPALIVE_WRITE_TIMEOUT: Duration = Duration::from_secs(5);
const LATENCY_PACKET: [u8; 33] = [
    0x12, 0, 0, 3, 0xe8, 0, 0, 0, 0x18, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0,
];

pub async fn receive_stream(
    stream: impl AsyncRead + AsyncWrite + Unpin,
    publisher: &PublisherGuard,
) -> Result<bool, LiveError> {
    // Errors and cancellation drop both TLS halves and synchronously revoke audio.
    let (mut reader, mut writer) = tokio::io::split(stream);
    let (connection, mut audio_frames) = publisher.audio().connect();
    let mut tick = tokio::time::interval_at(
        tokio::time::Instant::now() + Duration::from_secs(1),
        Duration::from_secs(1),
    );
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut keepalive = Keepalive::default();
    let mut audio = Uplink::default();
    let mut decoder = ImmiDecoder::default();
    let offer = AudioOffer::default();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    let mut media_seen = false;
    loop {
        if !publisher.has_subscribers() {
            return Ok(media_seen);
        }
        let read = tokio::select! {
            read = reader.read(&mut buffer) => read?,
            frame = audio_frames.recv() => {
                let Some(frame) = frame else { return Ok(media_seen); };
                if !publisher.has_subscribers() { return Ok(media_seen); }
                audio.send(&mut writer, &connection, &frame).await?;
                continue;
            }
            _ = tick.tick() => {
                if !publisher.has_subscribers() { return Ok(media_seen); }
                keepalive.send(&mut writer).await?;
                continue;
            }
        };
        if read == 0 {
            break;
        }
        for event in decoder.push_events(&buffer[..read])? {
            match event {
                ImmiEvent::Video(frame) => {
                    media_seen = true;
                    publisher.publish(frame);
                }
                ImmiEvent::AudioConfig(format) => {
                    // Every offer updates authorization; only the first is logged.
                    if connection.observe_offer(format).is_err() {
                        return Ok(media_seen);
                    }
                    if !offer.snapshot().offered {
                        offer.observe(format);
                        tracing::info!(offer = ?offer.snapshot(), "Blink IMMI audio offer");
                    }
                }
            }
        }
    }
    offer.clear();
    if media_seen {
        decoder.finish()?;
    }
    Ok(media_seen)
}

#[derive(Default)]
struct Uplink {
    epoch: Option<u64>,
    packets: AudioPacketWriter,
}
impl Uplink {
    async fn send(
        &mut self,
        writer: &mut (impl AsyncWrite + Unpin),
        connection: &AudioConnection,
        frame: &AudioFrame,
    ) -> Result<(), LiveError> {
        if !connection.valid_frame(frame, Instant::now()) {
            return Ok(());
        }
        if self.epoch != Some(frame.epoch()) {
            self.packets = AudioPacketWriter::default();
            self.epoch = Some(frame.epoch());
        }
        // Serialization is synchronous; nothing may await after authorization.
        if !connection.valid_frame(frame, Instant::now()) {
            return Ok(());
        }
        let packet = self
            .packets
            .audio_frame(frame.payload())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid AAC frame"))?;
        let Some(remaining) = frame.remaining(Instant::now()) else {
            return Ok(());
        };
        // Socket backpressure must not extend the original PCM freshness budget.
        bounded_write(writer, &packet, remaining).await?;
        connection.frame_sent(frame);
        Ok(())
    }
}

#[derive(Default)]
struct Keepalive {
    ticks: u32,
    sequence: u32,
}
impl Keepalive {
    async fn send(&mut self, writer: &mut (impl AsyncWrite + Unpin)) -> Result<(), LiveError> {
        self.ticks = self.ticks.wrapping_add(1);
        let mut packet = Vec::with_capacity(42);
        if self.ticks.is_multiple_of(10) {
            self.sequence = self.sequence.wrapping_add(1);
            packet.push(0x0a);
            packet.extend_from_slice(&self.sequence.to_be_bytes());
            packet.extend_from_slice(&[0; 4]);
        }
        packet.extend_from_slice(&LATENCY_PACKET);
        bounded_write(writer, &packet, KEEPALIVE_WRITE_TIMEOUT).await
    }
}

async fn bounded_write(
    writer: &mut (impl AsyncWrite + Unpin),
    packet: &[u8],
    duration: Duration,
) -> Result<(), LiveError> {
    tokio::time::timeout(duration, async {
        writer.write_all(packet).await?;
        writer.flush().await
    })
    .await
    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "IMMI write timed out"))??;
    Ok(())
}

#[cfg(test)]
#[path = "live_audio_tests.rs"]
mod tests;
