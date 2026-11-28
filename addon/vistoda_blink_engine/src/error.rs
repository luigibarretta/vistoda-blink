use axum::{
    Json,
    http::{HeaderValue, StatusCode, header::RETRY_AFTER},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use thiserror::Error;

use crate::blink_client::BlinkError;

#[derive(Debug, Error)]
pub enum EngineError {
    /// The local bridge bearer token is missing or wrong (HTTP 401).
    #[error("authentication required")]
    Unauthorized,
    /// Blink revoked the stored authorization and a refresh cannot fix it:
    /// the user must sign in again (HTTP 403, machine-readable body).
    #[error("reauth_required")]
    ReauthRequired,
    #[error("invalid camera alias")]
    InvalidAlias,
    #[error("camera publisher already active")]
    PublisherBusy,
    /// Blink runs one command per system and refused this one with HTTP 409 (for example
    /// because a live view is open).
    #[error("Blink is busy with another operation on this system")]
    ProviderBusy,
    /// Blink refused a live view a moment ago; the value is the seconds left to wait.
    #[error("Blink refused the live view; try again in a moment")]
    LiveCoolingDown(u64),
    #[error("invalid IMMI frame: {0}")]
    Protocol(String),
    #[error("websocket transport failed: {0}")]
    Transport(String),
    #[error("Vistoda Blink is not enrolled")]
    NotEnrolled,
    #[error("Blink camera was not found")]
    CameraNotFound,
    #[error("Blink camera requires the legacy live transport")]
    WebRtcLegacyDevice,
    #[error("Blink WebRTC live is disabled by the official app policy")]
    WebRtcFeatureDisabled,
    #[error("Blink network was not found")]
    NetworkNotFound,
    #[error("Blink program was not found")]
    ProgramNotFound,
    #[error("Blink cloud request failed")]
    Cloud,
    #[error("enrollment request is invalid or expired")]
    InvalidEnrollment,
    #[error("camera setting is invalid or unsupported")]
    InvalidSetting,
    #[error("camera settings changed; reload before retrying")]
    SettingsConflict,
    #[error("camera setting could not be verified and was restored")]
    SettingsVerification,
    #[error("recording request is invalid")]
    RecordingInvalid,
    #[error("page or page size is invalid")]
    InvalidPage,
    #[error("a recording is already active for this camera")]
    RecordingActive,
    #[error("recording quota does not have safe headroom")]
    RecordingCapacity,
    #[error("recording storage is unavailable")]
    RecordingIo,
    #[error("recording was not found")]
    RecordingNotFound,
    #[error("local storage operation is invalid for the current support")]
    InvalidStorageOperation,
    #[error("provider-managed recording requires an active compatible live session")]
    ProviderRecordingUnavailable,
    #[error("Blink rejected the Sync Module command")]
    ProviderCommandFailed,
    #[error("Blink has not confirmed the Sync Module command yet")]
    ProviderCommandPending,
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

impl IntoResponse for EngineError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::ReauthRequired => StatusCode::FORBIDDEN,
            Self::InvalidAlias
            | Self::InvalidSetting
            | Self::InvalidStorageOperation
            | Self::Protocol(_)
            | Self::InvalidEnrollment => StatusCode::UNPROCESSABLE_ENTITY,
            Self::PublisherBusy
            | Self::ProviderBusy
            | Self::SettingsConflict
            | Self::RecordingActive
            | Self::ProviderRecordingUnavailable
            | Self::ProviderCommandFailed => StatusCode::CONFLICT,
            Self::ProviderCommandPending => StatusCode::GATEWAY_TIMEOUT,
            Self::RecordingCapacity | Self::LiveCoolingDown(_) => StatusCode::TOO_MANY_REQUESTS,
            Self::NotEnrolled => StatusCode::PRECONDITION_REQUIRED,
            Self::WebRtcLegacyDevice | Self::WebRtcFeatureDisabled => {
                StatusCode::PRECONDITION_FAILED
            }
            Self::CameraNotFound
            | Self::NetworkNotFound
            | Self::ProgramNotFound
            | Self::RecordingNotFound => StatusCode::NOT_FOUND,
            Self::Transport(_) | Self::Cloud | Self::SettingsVerification | Self::RecordingIo => {
                StatusCode::BAD_GATEWAY
            }
            Self::RecordingInvalid | Self::InvalidPage => StatusCode::BAD_REQUEST,
        };
        let retry_after = match self {
            Self::LiveCoolingDown(seconds) => Some(seconds),
            _ => None,
        };
        let mut response = (
            status,
            Json(ErrorBody {
                error: self.to_string(),
            }),
        )
            .into_response();
        if let Some(seconds) = retry_after {
            response
                .headers_mut()
                .insert(RETRY_AFTER, HeaderValue::from(seconds));
        }
        response
    }
}

impl From<BlinkError> for EngineError {
    fn from(error: BlinkError) -> Self {
        if error.http_status() == Some(axum::http::StatusCode::CONFLICT) {
            return Self::ProviderBusy;
        }
        match error {
            BlinkError::Authentication => Self::ReauthRequired,
            BlinkError::NotEnrolled => Self::NotEnrolled,
            BlinkError::CameraNotFound => Self::CameraNotFound,
            BlinkError::NetworkNotFound => Self::NetworkNotFound,
            BlinkError::ProgramNotFound => Self::ProgramNotFound,
            BlinkError::InvalidSetting | BlinkError::SettingsUnsupported => Self::InvalidSetting,
            BlinkError::SettingsConflict => Self::SettingsConflict,
            BlinkError::SettingsVerification => Self::SettingsVerification,
            BlinkError::InvalidStorageOperation => Self::InvalidStorageOperation,
            BlinkError::CommandFailed => Self::ProviderCommandFailed,
            BlinkError::CommandTimeout => Self::ProviderCommandPending,
            _ => Self::Cloud,
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::response::IntoResponse;

    use super::EngineError;
    use crate::blink_client::BlinkError;

    #[tokio::test]
    async fn blink_and_bridge_authentication_failures_are_distinct() {
        let bridge = EngineError::Unauthorized.into_response();
        assert_eq!(bridge.status(), axum::http::StatusCode::UNAUTHORIZED);
        let blink = EngineError::from(BlinkError::Authentication).into_response();
        assert_eq!(blink.status(), axum::http::StatusCode::FORBIDDEN);
        let body = axum::body::to_bytes(blink.into_body(), 1024).await;
        assert_eq!(
            body.ok().as_deref(),
            Some(br#"{"error":"reauth_required"}"#.as_slice())
        );
        let cloud = EngineError::from(BlinkError::InvalidResponse).into_response();
        assert_eq!(cloud.status(), axum::http::StatusCode::BAD_GATEWAY);
    }

    #[allow(clippy::expect_used)] // A hand-made test response cannot fail to build.
    fn rejected_by_blink(status: u16) -> BlinkError {
        let response = reqwest::Response::from(
            axum::http::Response::builder()
                .status(status)
                .body("")
                .expect("a valid test response"),
        );
        BlinkError::Transport(response.error_for_status().expect_err("an error status"))
    }

    #[test]
    fn a_command_blink_refuses_as_busy_is_a_409_not_a_bad_gateway() {
        // Blink answers 409 to arm or snapshot while a live view is open.
        let busy = EngineError::from(rejected_by_blink(409));
        assert!(matches!(busy, EngineError::ProviderBusy));
        assert_eq!(
            busy.into_response().status(),
            axum::http::StatusCode::CONFLICT
        );
        for status in [400, 404, 429, 500, 503] {
            let other = EngineError::from(rejected_by_blink(status)).into_response();
            assert_eq!(
                other.status(),
                axum::http::StatusCode::BAD_GATEWAY,
                "{status}"
            );
        }
    }

    #[test]
    fn legacy_webrtc_precondition_has_a_dedicated_status() {
        let response = EngineError::WebRtcLegacyDevice.into_response();
        assert_eq!(
            response.status(),
            axum::http::StatusCode::PRECONDITION_FAILED
        );
    }

    #[test]
    fn disabled_webrtc_feature_uses_the_legacy_precondition_status() {
        let response = EngineError::WebRtcFeatureDisabled.into_response();
        assert_eq!(
            response.status(),
            axum::http::StatusCode::PRECONDITION_FAILED
        );
    }
}
