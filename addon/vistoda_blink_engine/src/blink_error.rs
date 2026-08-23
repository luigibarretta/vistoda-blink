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
    #[error("Blink command timed out")]
    CommandTimeout,
    #[error("Blink media exceeded its safety limit")]
    MediaTooLarge,
}
