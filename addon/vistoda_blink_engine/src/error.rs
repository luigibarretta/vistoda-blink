use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use thiserror::Error;

use crate::blink_client::BlinkError;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("authentication required")]
    Unauthorized,
    #[error("invalid camera alias")]
    InvalidAlias,
    #[error("camera publisher already active")]
    PublisherBusy,
    #[error("invalid IMMI frame: {0}")]
    Protocol(String),
    #[error("websocket transport failed: {0}")]
    Transport(String),
    #[error("Vistoda Blink is not enrolled")]
    NotEnrolled,
    #[error("Blink camera was not found")]
    CameraNotFound,
    #[error("Blink network was not found")]
    NetworkNotFound,
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
    #[error("a recording is already active for this camera")]
    RecordingActive,
    #[error("recording quota does not have safe headroom")]
    RecordingCapacity,
    #[error("recording storage is unavailable")]
    RecordingIo,
    #[error("recording was not found")]
    RecordingNotFound,
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

impl IntoResponse for EngineError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::InvalidAlias
            | Self::InvalidSetting
            | Self::Protocol(_)
            | Self::InvalidEnrollment => StatusCode::UNPROCESSABLE_ENTITY,
            Self::PublisherBusy | Self::SettingsConflict | Self::RecordingActive => {
                StatusCode::CONFLICT
            }
            Self::RecordingCapacity => StatusCode::TOO_MANY_REQUESTS,
            Self::NotEnrolled => StatusCode::PRECONDITION_REQUIRED,
            Self::CameraNotFound | Self::NetworkNotFound | Self::RecordingNotFound => {
                StatusCode::NOT_FOUND
            }
            Self::Transport(_) | Self::Cloud | Self::SettingsVerification | Self::RecordingIo => {
                StatusCode::BAD_GATEWAY
            }
            Self::RecordingInvalid => StatusCode::BAD_REQUEST,
        };
        (
            status,
            Json(ErrorBody {
                error: self.to_string(),
            }),
        )
            .into_response()
    }
}

impl From<BlinkError> for EngineError {
    fn from(error: BlinkError) -> Self {
        match error {
            BlinkError::Authentication => Self::Unauthorized,
            BlinkError::NotEnrolled => Self::NotEnrolled,
            BlinkError::CameraNotFound => Self::CameraNotFound,
            BlinkError::NetworkNotFound => Self::NetworkNotFound,
            BlinkError::InvalidSetting | BlinkError::SettingsUnsupported => Self::InvalidSetting,
            BlinkError::SettingsConflict => Self::SettingsConflict,
            BlinkError::SettingsVerification => Self::SettingsVerification,
            _ => Self::Cloud,
        }
    }
}
