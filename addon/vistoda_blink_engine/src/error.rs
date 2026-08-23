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
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

impl IntoResponse for EngineError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::InvalidAlias | Self::Protocol(_) | Self::InvalidEnrollment => {
                StatusCode::UNPROCESSABLE_ENTITY
            }
            Self::PublisherBusy => StatusCode::CONFLICT,
            Self::NotEnrolled => StatusCode::PRECONDITION_REQUIRED,
            Self::CameraNotFound | Self::NetworkNotFound => StatusCode::NOT_FOUND,
            Self::Transport(_) | Self::Cloud => StatusCode::BAD_GATEWAY,
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
            _ => Self::Cloud,
        }
    }
}
