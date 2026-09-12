//! Safe audio status: counters contain no samples, credentials or device IDs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AudioStatus {
    pub connected: bool,
    pub format: Option<u32>,
    pub microphone_enabled: bool,
    pub sent_frames: u64,
    pub multi_client: Option<bool>,
    /// Session availability, never a correlated ownership grant.
    pub audio_available: Option<bool>,
}

impl AudioStatus {
    /// Eligibility only. An enabled local owner is not a remote ownership grant.
    pub fn supported(&self) -> bool {
        self.connected
            && matches!(self.format, Some(0xa000_0001 | 0xa000_0003))
            && match self.multi_client {
                Some(false) => true,
                Some(true) => self.audio_available == Some(true),
                None => false,
            }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioLeaseError {
    Closed,
    UnsupportedOffer,
    Busy,
    Unavailable,
    Stale,
    InvalidFrame,
    Backpressure,
}
impl std::fmt::Display for AudioLeaseError {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(output, "Walnut audio lease: {self:?}")
    }
}
impl std::error::Error for AudioLeaseError {}
