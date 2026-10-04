use thiserror::Error;

use crate::{credentials::StoreError, oauth::OAuthError};

#[derive(Debug, Error)]
pub enum BlinkError {
    #[error("provider is not enrolled")]
    NotEnrolled,
    #[error("Blink authentication failed")]
    Authentication,
    #[error("Blink cloud transport failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("Blink credential storage failed: {0}")]
    Store(#[from] StoreError),
    #[error("Blink OAuth failed: {0}")]
    OAuth(#[from] OAuthError),
    #[error("Blink returned an invalid response")]
    InvalidResponse,
    #[error("Blink camera does not exist")]
    CameraNotFound,
    #[error("Blink network does not exist")]
    NetworkNotFound,
    #[error("Blink program does not exist")]
    ProgramNotFound,
    #[error("Blink command timed out")]
    CommandTimeout,
    #[error("Blink rejected the command")]
    CommandFailed,
    #[error("Blink media exceeded its safety limit")]
    MediaTooLarge,
    #[error("camera settings are not supported for this device")]
    SettingsUnsupported,
    #[error("camera setting value is invalid")]
    InvalidSetting,
    #[error("camera settings changed before the update")]
    SettingsConflict,
    #[error("camera setting verification failed")]
    SettingsVerification,
    #[error("local storage operation is invalid for the current support")]
    InvalidStorageOperation,
}

impl BlinkError {
    /// Blink answers HTTP 429, or 403 outside the OAuth refresh, when a client
    /// polls too often (ADR 0015).
    pub(crate) fn is_rate_limited(&self) -> bool {
        matches!(self, Self::Transport(error) if error.status().is_some_and(|status| {
            status == reqwest::StatusCode::TOO_MANY_REQUESTS || status == reqwest::StatusCode::FORBIDDEN
        }))
    }

    /// Only a rejected refresh grant needs a new sign-in; transport failures
    /// and unexpected token-endpoint answers stay retryable cloud errors.
    pub(crate) fn from_refresh(error: OAuthError) -> Self {
        match error {
            OAuthError::InvalidCredentials => Self::Authentication,
            other => Self::OAuth(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BlinkError;
    use crate::oauth::OAuthError;

    #[test]
    fn only_a_rejected_refresh_grant_is_an_authentication_failure() {
        assert!(matches!(
            BlinkError::from_refresh(OAuthError::InvalidCredentials),
            BlinkError::Authentication
        ));
        assert!(matches!(
            BlinkError::from_refresh(OAuthError::Unexpected),
            BlinkError::OAuth(_)
        ));
    }
}
