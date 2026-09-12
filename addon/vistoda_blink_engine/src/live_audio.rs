//! One future owns IMMI transport, audio authorization and bounded writes.
use crate::{
    framing::{ImmiDecoder, ImmiEvent},
    hub::PublisherGuard,
    immi_audio::AudioOffer,
    immi_audio_lease::AudioConnection,
    live::LiveError,
};
use std::{io, time::Duration};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const KEEPALIVE_WRITE_TIMEOUT: Duration = Duration::from_secs(5);
#[path = "live_audio_uplink.rs"]
mod uplink;
use uplink::Uplink;
const LATENCY_PACKET: [u8; 33] = [
    0x12, 0, 0, 3, 0xe8, 0, 0, 0, 0x18, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0,
];

pub async fn receive_stream(
    stream: impl AsyncRead + AsyncWrite + Unpin,
    publisher: &PublisherGuard,
    multi_client: Option<bool>,
) -> Result<bool, LiveError> {
    // Errors and cancellation drop both TLS halves and synchronously revoke audio.
    let (mut reader, mut writer) = tokio::io::split(stream);
    let (connection, mut audio_frames) = publisher.audio().connect_with_policy(multi_client);
    let mut changes = connection.changes();
    let mut tick = tokio::time::interval_at(
        tokio::time::Instant::now() + Duration::from_secs(1),
        Duration::from_secs(1),
    );
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut keepalive = Keepalive::default();
    let mut audio = Uplink::default();
    let mut decoder = ImmiDecoder::default();
    let mut observation = Observation::default();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        if !publisher.has_subscribers() || connection.control().is_none() {
            return Ok(observation.media_seen);
        }
        changes.borrow_and_update();
        audio.reconcile(&mut writer, &connection).await?;
        let read = tokio::select! {
            notification = changes.changed() => {
                if notification.is_err() { return Ok(observation.media_seen); }
                continue;
            }
            read = reader.read(&mut buffer) => read?,
            frame = audio_frames.recv() => {
                let Some(frame) = frame else { return Ok(observation.media_seen); };
                if !publisher.has_subscribers() { return Ok(observation.media_seen); }
                audio.send(&mut writer, &connection, &frame).await?;
                continue;
            }
            _ = tick.tick() => {
                if !publisher.has_subscribers() { return Ok(observation.media_seen); }
                let budget = audio.keepalive_budget(&connection);
                keepalive.send(&mut writer, budget).await?;
                continue;
            }
        };
        if read == 0 {
            break;
        }
        observation.process(
            decoder.push_events(&buffer[..read])?,
            publisher,
            &connection,
        )?;
    }
    observation.offer.clear();
    if observation.media_seen {
        decoder.finish()?;
    }
    Ok(observation.media_seen)
}

#[derive(Default)]
struct Observation {
    offer: AudioOffer,
    media_seen: bool,
    availability: Option<bool>,
}
impl Observation {
    fn process(
        &mut self,
        events: Vec<ImmiEvent>,
        publisher: &PublisherGuard,
        connection: &AudioConnection,
    ) -> Result<(), LiveError> {
        for event in events {
            match event {
                ImmiEvent::AudioAvailability(available) => {
                    connection
                        .observe_availability(available)
                        .map_err(io::Error::other)?;
                    // Availability is not a per-client ownership grant.
                    if self.availability != Some(available) {
                        self.availability = Some(available);
                        tracing::info!(available, "Blink session audio availability observation");
                    }
                }
                ImmiEvent::Video(frame) => {
                    self.media_seen = true;
                    publisher.publish(frame);
                }
                ImmiEvent::AudioConfig(format) => {
                    // Every offer updates authorization; only the first is logged.
                    connection.observe_offer(format).map_err(io::Error::other)?;
                    if !self.offer.snapshot().offered {
                        self.offer.observe(format);
                        tracing::info!(offer = ?self.offer.snapshot(), "Blink IMMI audio offer");
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Default)]
struct Keepalive {
    ticks: u32,
    sequence: u32,
}
impl Keepalive {
    async fn send(
        &mut self,
        writer: &mut (impl AsyncWrite + Unpin),
        budget: Duration,
    ) -> Result<(), LiveError> {
        self.ticks = self.ticks.wrapping_add(1);
        let mut packet = Vec::with_capacity(42);
        if self.ticks.is_multiple_of(10) {
            self.sequence = self.sequence.wrapping_add(1);
            packet.push(0x0a);
            packet.extend_from_slice(&self.sequence.to_be_bytes());
            packet.extend_from_slice(&[0; 4]);
        }
        packet.extend_from_slice(&LATENCY_PACKET);
        bounded_write(writer, &packet, budget).await
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
#[path = "live_audio_keepalive_tests.rs"]
mod keepalive_tests;
#[cfg(test)]
#[path = "live_audio_session_tests.rs"]
mod session_tests;
#[cfg(test)]
#[path = "live_audio_tests.rs"]
mod tests;
