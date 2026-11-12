//! The sealed provider store (`provider.sealed`), through the shared
//! lib-vistoda-provider-kit `sealed_store`.
//!
//! The store is sealed with the HKDF sub-key `vistoda-blink/provider-store/v1`
//! of the random `store.key` next to it (ADR-0016). Stores sealed by earlier
//! releases with `SHA-256("vistoda-blink-provider-store-v1" || 0x00 ||
//! workload token)` are opened with that legacy key once and re-sealed.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use vistoda_provider_kit::{
    KeyLabel,
    private_file::{self, FileError},
    sealed_store::{KeyFile, SealError, SealedFile, SealingKey},
};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

pub const PROVIDER_STORE: KeyLabel = KeyLabel::new("vistoda-blink/provider-store/v1");
pub const LEGACY_STORE_LABEL: &str = "vistoda-blink-provider-store-v1";

#[derive(Clone, Deserialize, Serialize, Zeroize, ZeroizeOnDrop)]
pub struct ProviderCredentials {
    pub refresh_token: String,
    pub hardware_id: String,
    pub region_id: Option<String>,
    pub account_id: Option<String>,
    pub user_id: Option<String>,
    pub username: Option<String>,
}

#[derive(Clone)]
pub struct CredentialStore {
    file: SealedFile,
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("credential store I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("credential store encoding is invalid")]
    Encoding,
    #[error("credential store payload is invalid: {0}")]
    Payload(#[from] serde_json::Error),
    #[error("credential store is unusable: {0}")]
    Sealed(#[from] SealError),
    #[error("private file rejected: {0}")]
    PrivateFile(#[from] FileError),
}

impl CredentialStore {
    /// `workload_token` is only used to open a store sealed before the
    /// dedicated key existed; such a store is re-sealed on first load.
    pub fn new(path: PathBuf, workload_token: &str) -> Self {
        let legacy =
            SealingKey::from_label_and_material(LEGACY_STORE_LABEL, workload_token.as_bytes());
        let key = KeyFile::beside(&path, PROVIDER_STORE);
        Self {
            file: SealedFile::new(path, key).with_legacy_key(legacy),
        }
    }

    pub async fn exists(&self) -> bool {
        tokio::fs::try_exists(self.file.path())
            .await
            .unwrap_or(false)
    }

    pub async fn load(&self) -> Result<Option<ProviderCredentials>, StoreError> {
        let file = self.file.clone();
        let Some(plaintext) = blocking(move || file.load()).await?? else {
            return Ok(None);
        };
        Ok(Some(serde_json::from_slice(&plaintext)?))
    }

    pub async fn save(&self, credentials: &ProviderCredentials) -> Result<(), StoreError> {
        let plaintext = Zeroizing::new(serde_json::to_vec(credentials)?);
        let file = self.file.clone();
        Ok(blocking(move || file.save(&plaintext)).await??)
    }
}

/// Atomically replaces `path` with a 0600 file, off the async runtime.
pub(crate) async fn write_private(path: PathBuf, bytes: Vec<u8>) -> Result<(), StoreError> {
    let bytes = Zeroizing::new(bytes);
    Ok(blocking(move || private_file::write_atomic(&path, &bytes)).await??)
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, StoreError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| StoreError::Io(std::io::Error::other(error)))
}

#[cfg(test)]
#[path = "credentials_tests.rs"]
mod tests;
