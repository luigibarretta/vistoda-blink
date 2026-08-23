use std::path::{Path, PathBuf};

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, AeadCore, KeyInit, OsRng},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::io::AsyncWriteExt;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

const STORE_VERSION: u8 = 1;

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
    key: [u8; 32],
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
}

#[derive(Deserialize, Serialize)]
struct Envelope {
    version: u8,
    nonce: String,
    ciphertext: String,
}

impl CredentialStore {
    pub fn new(path: PathBuf, workload_token: &str) -> Self {
        let mut digest = Sha256::new();
        digest.update(b"vistoda-blink-provider-store-v1\0");
        digest.update(workload_token.as_bytes());
        Self {
            path,
            key: digest.finalize().into(),
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
        if envelope.version != STORE_VERSION {
            return Err(StoreError::Encoding);
        }
        let nonce = STANDARD
            .decode(envelope.nonce)
            .map_err(|_| StoreError::Encoding)?;
        let ciphertext = STANDARD
            .decode(envelope.ciphertext)
            .map_err(|_| StoreError::Encoding)?;
        let nonce: [u8; 12] = nonce.try_into().map_err(|_| StoreError::Encoding)?;
        let plaintext = cipher(&self.key)
            .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
            .map_err(|_| StoreError::Authentication)?;
        let plaintext = Zeroizing::new(plaintext);
        Ok(Some(serde_json::from_slice(&plaintext)?))
    }

    pub async fn save(&self, credentials: &ProviderCredentials) -> Result<(), StoreError> {
        let plaintext = Zeroizing::new(serde_json::to_vec(credentials)?);
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = cipher(&self.key)
            .encrypt(&nonce, plaintext.as_ref())
            .map_err(|_| StoreError::Authentication)?;
        let envelope = Envelope {
            version: STORE_VERSION,
            nonce: STANDARD.encode(nonce),
            ciphertext: STANDARD.encode(ciphertext),
        };
        let payload = Zeroizing::new(serde_json::to_vec(&envelope)?);
        let temporary = temporary_path(&self.path);
        let mut file = tokio::fs::File::create(&temporary).await?;
        file.write_all(&payload).await?;
        file.sync_all().await?;
        drop(file);
        tokio::fs::rename(&temporary, &self.path).await?;
        if let Some(parent) = self.path.parent() {
            tokio::fs::File::open(parent).await?.sync_all().await?;
        }
        Ok(())
    }
}

fn cipher(key: &[u8; 32]) -> Aes256Gcm {
    Aes256Gcm::new_from_slice(key).unwrap_or_else(|_| unreachable!("fixed AES-256 key length"))
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut value = path.as_os_str().to_owned();
    value.push(".new");
    PathBuf::from(value)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::{CredentialStore, ProviderCredentials, StoreError};

    #[tokio::test]
    async fn round_trip_is_encrypted_and_key_bound() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("provider.sealed");
        let store = CredentialStore::new(path.clone(), &"a".repeat(64));
        let credentials = ProviderCredentials {
            refresh_token: "sensitive-refresh".into(),
            hardware_id: "hardware".into(),
            region_id: Some("prod".into()),
            account_id: Some("42".into()),
            user_id: None,
            username: Some("user@example.com".into()),
        };
        store.save(&credentials).await.expect("save credentials");
        let raw = tokio::fs::read_to_string(&path)
            .await
            .expect("read envelope");
        assert!(!raw.contains("sensitive-refresh"));
        let loaded = store
            .load()
            .await
            .expect("load credentials")
            .expect("present");
        assert_eq!(loaded.refresh_token, credentials.refresh_token);
        let wrong = CredentialStore::new(path, &"b".repeat(64));
        assert!(matches!(
            wrong.load().await,
            Err(StoreError::Authentication)
        ));
    }
}
