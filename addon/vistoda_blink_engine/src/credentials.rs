use std::path::PathBuf;

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, AeadCore, KeyInit, OsRng},
};
use auth_modules::secrets::files::{PrivateFileError, write_private_atomic};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::store_key::StoreKey;

/// Sealed with the dedicated `store.key` sub-key (`store_key.rs`).
const STORE_VERSION: u8 = 2;
/// Sealed with the legacy key derived from the workload token; opened once to
/// migrate, never written.
const LEGACY_STORE_VERSION: u8 = 1;

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
    path: PathBuf,
    key: StoreKey,
    legacy_key: Zeroizing<[u8; 32]>,
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("credential store I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("credential store encoding is invalid")]
    Encoding,
    #[error("credential store authentication failed")]
    Authentication,
    #[error("credential store payload is invalid: {0}")]
    Payload(#[from] serde_json::Error),
    #[error("private file rejected: {0}")]
    PrivateFile(#[from] PrivateFileError),
    #[error("provider store key is unusable: {0}")]
    Key(String),
}

#[derive(Deserialize, Serialize)]
struct Envelope {
    version: u8,
    nonce: String,
    ciphertext: String,
}

impl CredentialStore {
    /// `workload_token` is only used to open a store sealed before the
    /// dedicated key existed; such a store is re-sealed on first load.
    pub fn new(path: PathBuf, workload_token: &str) -> Self {
        let mut digest = Sha256::new();
        digest.update(b"vistoda-blink-provider-store-v1\0");
        digest.update(workload_token.as_bytes());
        Self {
            key: StoreKey::beside(&path),
            path,
            legacy_key: Zeroizing::new(digest.finalize().into()),
        }
    }

    pub async fn exists(&self) -> bool {
        tokio::fs::try_exists(&self.path).await.unwrap_or(false)
    }

    pub async fn load(&self) -> Result<Option<ProviderCredentials>, StoreError> {
        let content = match tokio::fs::read(&self.path).await {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let envelope: Envelope = serde_json::from_slice(&content)?;
        let key = match envelope.version {
            STORE_VERSION => self.key.existing().await?,
            LEGACY_STORE_VERSION => self.legacy_key.clone(),
            _ => return Err(StoreError::Encoding),
        };
        let plaintext = open(&key, &envelope.nonce, &envelope.ciphertext)?;
        let credentials: ProviderCredentials = serde_json::from_slice(&plaintext)?;
        if envelope.version == LEGACY_STORE_VERSION {
            // Until this succeeds the legacy envelope stays and is retried on
            // the next load; the credentials themselves are usable either way.
            if let Err(error) = self.save(&credentials).await {
                tracing::warn!(%error, "provider store migration to store.key failed");
            }
        }
        Ok(Some(credentials))
    }

    pub async fn save(&self, credentials: &ProviderCredentials) -> Result<(), StoreError> {
        let key = self.key.for_sealing().await?;
        let plaintext = Zeroizing::new(serde_json::to_vec(credentials)?);
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = cipher(&key)
            .encrypt(&nonce, plaintext.as_ref())
            .map_err(|_| StoreError::Authentication)?;
        let envelope = Envelope {
            version: STORE_VERSION,
            nonce: STANDARD.encode(nonce),
            ciphertext: STANDARD.encode(ciphertext),
        };
        write_private(self.path.clone(), serde_json::to_vec(&envelope)?).await
    }
}

/// Atomically replaces `path` with a 0600 file (`auth_modules`), off the
/// async runtime.
pub(crate) async fn write_private(path: PathBuf, bytes: Vec<u8>) -> Result<(), StoreError> {
    let bytes = Zeroizing::new(bytes);
    tokio::task::spawn_blocking(move || write_private_atomic(&path, &bytes))
        .await
        .map_err(|error| StoreError::Io(std::io::Error::other(error)))?
        .map_err(StoreError::from)
}

fn open(key: &[u8; 32], nonce: &str, ciphertext: &str) -> Result<Zeroizing<Vec<u8>>, StoreError> {
    let nonce = STANDARD.decode(nonce).map_err(|_| StoreError::Encoding)?;
    let ciphertext = STANDARD
        .decode(ciphertext)
        .map_err(|_| StoreError::Encoding)?;
    let nonce: [u8; 12] = nonce.try_into().map_err(|_| StoreError::Encoding)?;
    cipher(key)
        .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
        .map(Zeroizing::new)
        .map_err(|_| StoreError::Authentication)
}

fn cipher(key: &[u8; 32]) -> Aes256Gcm {
    Aes256Gcm::new_from_slice(key).unwrap_or_else(|_| unreachable!("fixed AES-256 key length"))
}

#[cfg(test)]
#[path = "credentials_tests.rs"]
mod tests;
