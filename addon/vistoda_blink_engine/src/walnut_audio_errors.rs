//! Stable, non-sensitive error reasons for browser diagnostics.
use crate::immi_audio_lease::AudioLeaseError;
use std::io;

#[derive(Debug, Clone, Copy)]
pub enum Failure {
    EncoderUnavailable,
    PcmFormat,
    PcmRate,
    PcmBacklog,
    PcmWriteTimeout,
    EncoderFailed,
    InvalidAudio,
    MissingProvenance,
}
impl Failure {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::EncoderUnavailable => "encoder_unavailable",
            Self::PcmFormat => "pcm_format",
            Self::PcmRate => "pcm_rate",
            Self::PcmBacklog => "pcm_backlog",
            Self::PcmWriteTimeout => "pcm_write_timeout",
            Self::EncoderFailed => "encoder_failed",
            Self::InvalidAudio => "invalid_audio",
            Self::MissingProvenance => "missing_provenance",
        }
    }
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.reason())
    }
}
impl std::error::Error for Failure {}

pub fn reason(error: &io::Error) -> &'static str {
    if let Some(failure) = error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<Failure>())
    {
        return failure.reason();
    }
    match error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<AudioLeaseError>())
    {
        Some(AudioLeaseError::Busy) => "busy",
        Some(AudioLeaseError::Unavailable) => "unavailable",
        Some(AudioLeaseError::Closed) => "disconnected",
        Some(AudioLeaseError::UnsupportedOffer) => "unsupported_format",
        Some(AudioLeaseError::Stale) => "lease_revoked",
        Some(AudioLeaseError::Expired) => "audio_expired",
        Some(AudioLeaseError::InvalidFrame) => "invalid_audio",
        Some(AudioLeaseError::Backpressure) => "audio_backpressure",
        None => "audio_failed",
    }
}
