use std::{net::SocketAddr, path::PathBuf};

use clap::Parser;
use serde::Deserialize;
use thiserror::Error;
use zeroize::Zeroizing;

const TOKEN_BYTES: usize = 64;

#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Cli {
    #[arg(long, default_value = "0.0.0.0:8099")]
    pub listen: SocketAddr,
    #[arg(long, default_value = "/data/options.json")]
    pub config: PathBuf,
}

#[derive(Debug)]
pub struct AppConfig {
    pub token: Zeroizing<String>,
    pub credentials_path: PathBuf,
    pub max_recording_seconds: u64,
    pub max_recording_bytes: u64,
    pub recording_quota_bytes: u64,
}

#[derive(Deserialize)]
struct Options {
    token: String,
    #[serde(default = "default_recording_seconds")]
    max_recording_seconds: u64,
    #[serde(default = "default_recording_bytes")]
    max_recording_bytes: u64,
    #[serde(default = "default_recording_quota")]
    recording_quota_bytes: u64,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("could not read {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid JSON in {path}: {source}")]
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("token must contain exactly 64 lowercase hexadecimal characters")]
    InvalidToken,
    #[error("recording limits are outside the safe supported range")]
    InvalidRecordingLimits,
}

impl AppConfig {
    pub async fn load(path: PathBuf) -> Result<Self, ConfigError> {
        let content = tokio::fs::read(&path)
            .await
            .map_err(|source| ConfigError::Read {
                path: path.clone(),
                source,
            })?;
        let options: Options =
            serde_json::from_slice(&content).map_err(|source| ConfigError::Parse {
                path: path.clone(),
                source,
            })?;
        if options.token.len() != TOKEN_BYTES
            || !options
                .token
                .bytes()
                .all(|value| value.is_ascii_digit() || (b'a'..=b'f').contains(&value))
        {
            return Err(ConfigError::InvalidToken);
        }
        if !(5..=60).contains(&options.max_recording_seconds)
            || !(8 * 1024 * 1024..=128 * 1024 * 1024).contains(&options.max_recording_bytes)
            || !(options.max_recording_bytes..=2 * 1024 * 1024 * 1024)
                .contains(&options.recording_quota_bytes)
        {
            return Err(ConfigError::InvalidRecordingLimits);
        }
        Ok(Self {
            token: Zeroizing::new(options.token),
            credentials_path: path
                .parent()
                .unwrap_or_else(|| std::path::Path::new("/data"))
                .join("provider.sealed"),
            max_recording_seconds: options.max_recording_seconds,
            max_recording_bytes: options.max_recording_bytes,
            recording_quota_bytes: options.recording_quota_bytes,
        })
    }
}

const fn default_recording_seconds() -> u64 {
    60
}

const fn default_recording_bytes() -> u64 {
    96 * 1024 * 1024
}

const fn default_recording_quota() -> u64 {
    512 * 1024 * 1024
}
