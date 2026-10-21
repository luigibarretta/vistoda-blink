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

    /// Text safe for logs and `/v1/motion`: reqwest errors render their URL,
    /// which carries account, network and Sync Module IDs, so transport
    /// failures are reduced to the endpoint class, status and failure kind.
    pub fn redacted(&self) -> String {
        match self {
            Self::Transport(error) => {
                let endpoint = error.url().map_or("provider", |url| {
                    crate::blink_http::endpoint_class(url.path())
                });
                format!(
                    "Blink cloud request failed ({})",
                    transport_detail(endpoint, error)
                )
            }
            Self::OAuth(OAuthError::Transport(error)) => {
                format!(
                    "Blink OAuth request failed ({})",
                    transport_detail("oauth", error)
                )
            }
            other => other.to_string(),
        }
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

fn transport_detail(endpoint: &str, error: &reqwest::Error) -> String {
    if let Some(status) = error.status() {
        return format!("endpoint={endpoint}, status={}", status.as_u16());
    }
    let kind = if error.is_timeout() {
        "timeout"
    } else if error.is_connect() {
        "connect"
    } else if error.is_decode() {
        "decode"
    } else if error.is_body() {
        "body"
    } else if error.is_redirect() {
        "redirect"
    } else {
        "request"
    };
    format!("endpoint={endpoint}, kind={kind}")
}

#[cfg(test)]
mod tests {
    use super::BlinkError;
    use crate::oauth::OAuthError;

    #[tokio::test]
    async fn redacted_transport_errors_never_carry_the_url() {
        // Port 9 on loopback refuses the connection without leaving the host.
        let url = "http://127.0.0.1:9/api/v1/accounts/85085/media/changed?since=x";
        let Err(error) = reqwest::get(url).await else {
            return;
        };
        let error = BlinkError::Transport(error);
        assert!(error.to_string().contains("85085"));
        let redacted = error.redacted();
        assert!(!redacted.contains("85085") && !redacted.contains("127.0.0.1"));
        assert!(redacted.contains("endpoint=media"), "{redacted}");
        assert_eq!(
            BlinkError::InvalidResponse.redacted(),
            "Blink returned an invalid response"
        );
    }

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
