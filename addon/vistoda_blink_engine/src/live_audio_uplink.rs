//! Serialized microphone commands. Written Start is NOT a remote ownership grant.
use super::{KEEPALIVE_WRITE_TIMEOUT, bounded_write};
use crate::{
    immi_audio_lease::{AudioConnection, AudioFrame},
    immi_audio_wire::AudioPacketWriter,
    live::LiveError,
};
use std::{
    io,
    time::{Duration, Instant},
};
use tokio::io::AsyncWrite;

pub(super) const CONTROL_WRITE_TIMEOUT: Duration = Duration::from_millis(200);
const START: [u8; 9] = [0x17, 0, 0, 0, 3, 0, 0, 0, 0];
const STOP: [u8; 9] = [0x17, 0, 0, 0, 4, 0, 0, 0, 0];

#[derive(Default)]
pub(super) struct Uplink {
    epoch: Option<u64>,
    started_epoch: Option<u64>,
    packets: AudioPacketWriter,
}
impl Uplink {
    pub(super) fn keepalive_budget(&self, connection: &AudioConnection) -> Duration {
        // A revoked lease can still need Stop. The packet-counter epoch is not
        // sufficient: it intentionally survives Stop until the next activation.
        if self.started_epoch.is_some()
            || connection
                .control()
                .is_some_and(|(multi_client, desired)| multi_client && desired.is_some())
        {
            CONTROL_WRITE_TIMEOUT
        } else {
            KEEPALIVE_WRITE_TIMEOUT
        }
    }

    pub(super) async fn reconcile(
        &mut self,
        writer: &mut (impl AsyncWrite + Unpin),
        connection: &AudioConnection,
    ) -> Result<(), LiveError> {
        let Some((multi_client, desired)) = connection.control() else {
            return Ok(());
        };
        if !multi_client {
            return Ok(());
        }
        if self.started_epoch.is_some() && self.started_epoch != desired {
            self.stop(writer).await?;
        }
        // The previous stop may have awaited while ownership changed again.
        let Some((true, Some(epoch))) = connection.control() else {
            return Ok(());
        };
        if self.started_epoch == Some(epoch) {
            return Ok(());
        }
        bounded_write(writer, &START, CONTROL_WRITE_TIMEOUT).await?;
        self.started_epoch = Some(epoch);
        // A completed stale Start must be paired with Stop, never stale AAC.
        if connection.control() != Some((true, Some(epoch))) {
            self.stop(writer).await?;
        }
        Ok(())
    }

    async fn stop(&mut self, writer: &mut (impl AsyncWrite + Unpin)) -> Result<(), LiveError> {
        bounded_write(writer, &STOP, CONTROL_WRITE_TIMEOUT).await?;
        self.started_epoch = None;
        Ok(())
    }

    pub(super) async fn send(
        &mut self,
        writer: &mut (impl AsyncWrite + Unpin),
        connection: &AudioConnection,
        frame: &AudioFrame,
    ) -> Result<(), LiveError> {
        self.reconcile(writer, connection).await?;
        if !connection.valid_frame(frame, Instant::now()) {
            return Ok(());
        }
        if connection.control() == Some((true, Some(frame.epoch())))
            && self.started_epoch != Some(frame.epoch())
        {
            return Ok(());
        }
        if self.epoch != Some(frame.epoch()) {
            self.packets = AudioPacketWriter::default();
            self.epoch = Some(frame.epoch());
        }
        let mut candidate = self.packets;
        let packet = candidate
            .audio_frame(frame.payload())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid AAC frame"))?;
        self.write_frame(writer, connection, frame, candidate, &packet)
            .await
    }

    async fn write_frame(
        &mut self,
        writer: &mut (impl AsyncWrite + Unpin),
        connection: &AudioConnection,
        frame: &AudioFrame,
        candidate: AudioPacketWriter,
        packet: &[u8],
    ) -> Result<(), LiveError> {
        if !connection.valid_frame(frame, Instant::now()) {
            return Ok(());
        }
        let Some(remaining) = frame.remaining(Instant::now()) else {
            return Ok(());
        };
        // Never extend the original PCM age across control writes or backpressure.
        bounded_write(writer, packet, remaining).await?;
        self.packets = candidate;
        connection.frame_sent(frame);
        Ok(())
    }
}

#[cfg(test)]
#[path = "live_audio_control_tests.rs"]
mod tests;
