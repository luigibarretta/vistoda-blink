//! Connection-scoped observations and synchronous invalidation.
use super::{AudioConnection, AudioFrame, AudioLeaseError, fresh, policy_allows};
use std::time::Instant;
use tokio::sync::watch;

impl AudioConnection {
    pub(crate) fn changes(&self) -> watch::Receiver<()> {
        self.runtime.0.control.subscribe()
    }

    /// Current local desire, not remote ownership. None means this TLS is obsolete.
    pub(crate) fn control(&self) -> Option<(bool, Option<u64>)> {
        let state = self.runtime.state();
        (state.generation == self.generation && state.sender.is_some()).then(|| {
            (
                state.multi_client == Some(true),
                (state.owned && policy_allows(&state)).then_some(state.epoch),
            )
        })
    }

    pub fn observe_availability(&self, available: bool) -> Result<(), AudioLeaseError> {
        let mut state = self.runtime.state();
        if state.generation != self.generation || state.sender.is_none() {
            return Err(AudioLeaseError::Closed);
        }
        if state.available != Some(available) {
            state.available = Some(available);
            if !available && state.multi_client == Some(true) {
                state.epoch = state.epoch.wrapping_add(1);
                state.owned = false;
            }
            self.runtime.publish(&state);
        }
        Ok(())
    }

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
            if state.format.is_some() && state.multi_client == Some(true) {
                state.available = None;
            }
            state.format = Some(format);
            self.runtime.publish(&state);
        }
        Ok(())
    }

    /// Recheck immediately before writing, with no intervening queue or await.
    pub fn valid_frame(&self, frame: &AudioFrame, now: Instant) -> bool {
        let state = self.runtime.state();
        state.generation == self.generation
            && state.generation == frame.generation
            && state.epoch == frame.epoch
            && state.owned
            && state.sender.is_some()
            && policy_allows(&state)
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
            state.multi_client = None;
            state.available = None;
            self.runtime.publish(&state);
        }
    }
}
