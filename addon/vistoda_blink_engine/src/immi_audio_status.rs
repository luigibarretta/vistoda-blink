//! Safe audio status: counters contain no samples, credentials or device IDs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AudioStatus {
    pub connected: bool,
    pub format: Option<u32>,
    pub microphone_enabled: bool,
    pub sent_frames: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioLeaseError {
    Closed,
    UnsupportedOffer,
    Busy,
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
