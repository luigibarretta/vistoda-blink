//! Per-camera audio ownership. No provider request or microphone capture occurs here.
use crate::immi_audio_wire::AudioPacketWriter;
use std::{
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, watch};

pub const AUDIO_QUEUE_DEPTH: usize = 3;
pub const MAX_AUDIO_AGE: Duration = Duration::from_millis(200);

#[path = "immi_audio_status.rs"]
mod status;
pub use status::{AudioLeaseError, AudioStatus};

#[derive(Default)]
struct State {
    generation: u64,
    epoch: u64,
    format: Option<u32>,
    owned: bool,
    sent_frames: u64,
    sender: Option<mpsc::Sender<AudioFrame>>,
}
struct Inner {
    state: Mutex<State>,
    status: watch::Sender<AudioStatus>,
}
#[derive(Clone)]
pub struct AudioRuntime(Arc<Inner>);

impl Default for AudioRuntime {
    fn default() -> Self {
        Self(Arc::new(Inner {
            state: Mutex::new(State::default()),
            status: watch::channel(AudioStatus::default()).0,
        }))
    }
}

impl AudioRuntime {
    fn state(&self) -> MutexGuard<'_, State> {
        self.0
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn publish(&self, state: &State) {
        self.0.status.send_replace(AudioStatus {
            connected: state.sender.is_some(),
            format: state.format,
            microphone_enabled: state.owned,
            sent_frames: state.sent_frames,
        });
    }

    pub fn subscribe(&self) -> watch::Receiver<AudioStatus> {
        self.0.status.subscribe()
    }

    /// A reconnect invalidates all previous owners and queued-frame authorization.
    pub fn connect(&self) -> (AudioConnection, mpsc::Receiver<AudioFrame>) {
        let (sender, receiver) = mpsc::channel(AUDIO_QUEUE_DEPTH);
        let mut state = self.state();
        state.generation = state.generation.wrapping_add(1);
        state.epoch = state.epoch.wrapping_add(1);
        state.format = None;
        state.sent_frames = 0;
        state.owned = false;
        state.sender = Some(sender);
        self.publish(&state);
        (
            AudioConnection {
                runtime: self.clone(),
                generation: state.generation,
            },
            receiver,
        )
    }

    /// Call only after an explicit microphone gesture and browser authorization.
    pub fn claim(&self) -> Result<AudioLease, AudioLeaseError> {
        let mut state = self.state();
        if state.sender.is_none() {
            return Err(AudioLeaseError::Closed);
        }
        if !matches!(state.format, Some(0xa000_0001 | 0xa000_0003)) {
            return Err(AudioLeaseError::UnsupportedOffer);
        }
        if state.owned {
            return Err(AudioLeaseError::Busy);
        }
        state.epoch = state.epoch.wrapping_add(1);
        state.owned = true;
        state.sent_frames = 0;
        self.publish(&state);
        Ok(AudioLease {
            runtime: self.clone(),
            generation: state.generation,
            epoch: state.epoch,
        })
    }
}

pub struct AudioConnection {
    runtime: AudioRuntime,
    generation: u64,
}

impl AudioConnection {
    /// Count only completed TLS writes belonging to the current authorized epoch.
    pub fn frame_sent(&self, frame: &AudioFrame) {
        let mut state = self.runtime.state();
        if state.generation == self.generation && state.epoch == frame.epoch && state.owned {
            state.sent_frames = state.sent_frames.saturating_add(1);
            if state.sent_frames == 1 || state.sent_frames.is_multiple_of(16) {
                self.runtime.publish(&state);
            }
        }
    }
    pub fn observe_offer(&self, format: u32) -> Result<(), AudioLeaseError> {
        let mut state = self.runtime.state();
        if state.generation != self.generation || state.sender.is_none() {
            return Err(AudioLeaseError::Closed);
        }
        if state.format != Some(format) {
            state.epoch = state.epoch.wrapping_add(1);
            state.owned = false;
            state.format = Some(format);
            self.runtime.publish(&state);
        }
        Ok(())
    }

    /// Recheck immediately before writing, with no intervening queue or await.
    /// Recreate `AudioPacketWriter` whenever the accepted frame's epoch changes.
    pub fn valid_frame(&self, frame: &AudioFrame, now: Instant) -> bool {
        let state = self.runtime.state();
        state.generation == self.generation
            && state.generation == frame.generation
            && state.epoch == frame.epoch
            && state.owned
            && state.sender.is_some()
            && fresh(frame.created, now)
    }
}

impl Drop for AudioConnection {
    fn drop(&mut self) {
        let mut state = self.runtime.state();
        if state.generation == self.generation {
            state.epoch = state.epoch.wrapping_add(1);
            state.owned = false;
            state.format = None;
            state.sender = None;
            self.runtime.publish(&state);
        }
    }
}

pub struct AudioLease {
    runtime: AudioRuntime,
    generation: u64,
    epoch: u64,
}

impl AudioLease {
    /// Nonblocking: a full queue fails rather than buffering speech for later replay.
    pub fn submit(&self, bytes: Vec<u8>, created: Instant) -> Result<(), AudioLeaseError> {
        AudioPacketWriter::default()
            .audio_frame(&bytes)
            .map_err(|_| AudioLeaseError::InvalidFrame)?;
        let state = self.runtime.state();
        if state.generation != self.generation
            || state.epoch != self.epoch
            || !state.owned
            || !fresh(created, Instant::now())
        {
            return Err(AudioLeaseError::Stale);
        }
        let sender = state.sender.as_ref().ok_or(AudioLeaseError::Closed)?;
        sender
            .try_send(AudioFrame {
                bytes,
                created,
                generation: self.generation,
                epoch: self.epoch,
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => AudioLeaseError::Backpressure,
                mpsc::error::TrySendError::Closed(_) => AudioLeaseError::Closed,
            })
    }
}

impl Drop for AudioLease {
    fn drop(&mut self) {
        let mut state = self.runtime.state();
        if state.generation == self.generation && state.epoch == self.epoch && state.owned {
            state.epoch = state.epoch.wrapping_add(1);
            state.owned = false;
            self.runtime.publish(&state);
        }
    }
}

pub struct AudioFrame {
    bytes: Vec<u8>,
    created: Instant,
    generation: u64,
    epoch: u64,
}
impl AudioFrame {
    pub fn remaining(&self, now: Instant) -> Option<Duration> {
        MAX_AUDIO_AGE.checked_sub(now.checked_duration_since(self.created)?)
    }
    pub fn payload(&self) -> &[u8] {
        &self.bytes
    }
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
}

fn fresh(created: Instant, now: Instant) -> bool {
    now.checked_duration_since(created)
        .is_some_and(|age| age <= MAX_AUDIO_AGE)
}

#[cfg(test)]
#[path = "immi_audio_lease_tests.rs"]
mod tests;
