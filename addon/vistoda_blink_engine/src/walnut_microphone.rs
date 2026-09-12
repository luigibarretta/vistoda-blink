//! One explicitly enabled, bounded PCM -> AAC microphone epoch.
use crate::{
    audio_codec::AudioCodec,
    immi_audio_lease::{AudioLease, AudioLeaseError, AudioRuntime, MAX_AUDIO_AGE},
    immi_audio_wire::AdtsDecoder,
};
use std::{
    collections::VecDeque,
    io,
    time::{Duration, Instant},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const PCM_SAMPLES: u16 = 512;
#[path = "walnut_audio_errors.rs"]
pub mod errors;
use errors::Failure;

pub struct Microphone {
    codec: AudioCodec,
    lease: Option<AudioLease>,
    decoder: AdtsDecoder,
    samples_in: u64,
    provenance: Provenance,
    started: Instant,
    last_input: Instant,
    dropped_stale: u64,
    dropped_full: u64,
}

impl Microphone {
    pub fn start(runtime: &AudioRuntime) -> io::Result<Self> {
        let lease = runtime.claim().map_err(io::Error::other)?;
        Ok(Self {
            codec: AudioCodec::encoder()
                .map_err(|_| io::Error::other(Failure::EncoderUnavailable))?,
            lease: Some(lease),
            decoder: AdtsDecoder::default(),
            samples_in: 0,
            provenance: Provenance::default(),
            started: Instant::now(),
            last_input: Instant::now(),
            dropped_stale: 0,
            dropped_full: 0,
        })
    }

    pub async fn pcm(&mut self, bytes: &[u8]) -> io::Result<()> {
        // Exactly 32ms of signed little-endian, 16kHz, mono PCM. Timestamp
        // receipt before the encoder write; encoder output must never redate it.
        let created = Instant::now();
        let elapsed_samples = self.started.elapsed().as_millis() * 16;
        if bytes.len() != usize::from(PCM_SAMPLES) * 2 {
            return Err(io::Error::other(Failure::PcmFormat));
        }
        if u128::from(self.samples_in) + u128::from(PCM_SAMPLES)
            > elapsed_samples + u128::from(PCM_SAMPLES) * 2
        {
            return Err(io::Error::other(Failure::PcmRate));
        }
        if self.provenance.samples + usize::from(PCM_SAMPLES) > 4096 {
            return Err(io::Error::other(Failure::PcmBacklog));
        }
        tokio::time::timeout(
            Duration::from_millis(100),
            self.codec.input.write_all(bytes),
        )
        .await
        .map_err(|_| io::Error::other(Failure::PcmWriteTimeout))?
        .map_err(|_| io::Error::other(Failure::EncoderFailed))?;
        self.provenance.record(usize::from(PCM_SAMPLES), created)?;
        self.samples_in += u64::from(PCM_SAMPLES);
        self.last_input = created;
        Ok(())
    }

    pub async fn forward(&mut self) -> io::Result<()> {
        let mut buffer = [0_u8; 8192];
        let length = self
            .codec
            .output
            .read(&mut buffer)
            .await
            .map_err(|_| io::Error::other(Failure::EncoderFailed))?;
        if length == 0 {
            return Err(io::Error::other(Failure::EncoderFailed));
        }
        let frames = self
            .decoder
            .push(&buffer[..length])
            .map_err(|_| io::Error::other(Failure::InvalidAudio))?;
        for frame in frames {
            let Some(created) = self.provenance.take(Instant::now())? else {
                // FFmpeg AAC emits one 1024-sample priming frame, not PCM data.
                continue;
            };
            if !fresh(created, Instant::now()) {
                note_drop(&mut self.dropped_stale, "stale");
                continue;
            }
            let result = self
                .lease
                .as_ref()
                .ok_or_else(|| io::Error::other(AudioLeaseError::Stale))?
                .submit(frame, created);
            handle_submit(result, &mut self.dropped_stale, &mut self.dropped_full)?;
        }
        Ok(())
    }

    pub fn idle(&self) -> bool {
        self.last_input.elapsed() > Duration::from_millis(500)
    }

    pub async fn stop(mut self) {
        // Revoke wire authorization BEFORE awaiting process cleanup.
        self.lease.take();
        self.codec.stop().await;
    }
}

/// FIFO input provenance; partial chunks retain their original receipt time.
/// `FFmpeg`'s pinned AAC encoder declares `frame_size=initial_padding=1024`.
struct Provenance {
    pending: VecDeque<(usize, Instant)>,
    samples: usize,
    priming: bool,
}
impl Default for Provenance {
    fn default() -> Self {
        Self {
            pending: VecDeque::new(),
            samples: 0,
            priming: true,
        }
    }
}
impl Provenance {
    fn record(&mut self, samples: usize, created: Instant) -> io::Result<()> {
        if samples == 0 || self.samples + samples > 4096 {
            return Err(io::Error::other("PCM provenance budget exceeded"));
        }
        self.pending.push_back((samples, created));
        self.samples += samples;
        Ok(())
    }
    fn take(&mut self, _now: Instant) -> io::Result<Option<Instant>> {
        if self.priming {
            self.priming = false;
            return Ok(None);
        }
        if self.samples < 1024 {
            return Err(io::Error::other(Failure::MissingProvenance));
        }
        let created = self
            .pending
            .front()
            .map(|(_, instant)| *instant)
            .ok_or_else(|| io::Error::other("missing PCM"))?;
        let mut remaining = 1024;
        while remaining > 0 {
            let (samples, _) = self
                .pending
                .front_mut()
                .ok_or_else(|| io::Error::other("missing PCM"))?;
            let consumed = remaining.min(*samples);
            *samples -= consumed;
            remaining -= consumed;
            self.samples -= consumed;
            if *samples == 0 {
                self.pending.pop_front();
            }
        }
        Ok(Some(created))
    }
}

fn fresh(created: Instant, now: Instant) -> bool {
    now.checked_duration_since(created)
        .is_some_and(|age| age <= MAX_AUDIO_AGE)
}

fn note_drop(counter: &mut u64, reason: &'static str) {
    *counter = counter.saturating_add(1);
    if *counter == 1 || (*counter).is_multiple_of(16) {
        tracing::warn!(
            reason,
            dropped_frames = *counter,
            "Blink microphone dropped unsent AAC"
        );
    }
}

fn handle_submit(
    result: Result<(), AudioLeaseError>,
    stale: &mut u64,
    full: &mut u64,
) -> io::Result<()> {
    match result {
        Ok(()) => {}
        Err(AudioLeaseError::Backpressure) => note_drop(full, "queue_full"),
        Err(AudioLeaseError::Expired) => note_drop(stale, "stale"),
        // Expiry can race submit; never hide an actual lease revocation.
        Err(error) => return Err(io::Error::other(error)),
    }
    Ok(())
}

pub async fn forward(microphone: &mut Option<Microphone>) -> io::Result<()> {
    match microphone {
        Some(value) => value.forward().await,
        None => std::future::pending().await,
    }
}

#[cfg(test)]
#[path = "walnut_microphone_tests.rs"]
mod tests;
