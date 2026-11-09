//! Dedicated key of the provider credential store (`store.key`).
//!
//! `provider.sealed` used to be encrypted with `SHA-256(label || workload
//! token)`. The workload token is also the API bearer that the Home Assistant
//! adapter receives through discovery, so anyone holding the bearer could
//! decrypt the Blink refresh token. The store is now sealed with an HKDF
//! sub-key (`vistoda-blink/provider-store/v1`) of 32 random bytes kept in a
//! separate owner-private file that never leaves the app.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use aes_gcm::aead::{OsRng, rand_core::RngCore};
use auth_modules::keys::{KeyLabel, MasterKey, SUBKEY_BYTES};
use auth_modules::secrets::files::{PrivateFileError, read_private_file, write_private_atomic};
use tokio::sync::OnceCell;
use zeroize::Zeroizing;

use crate::credentials::StoreError;

pub const STORE_KEY_FILE: &str = "store.key";
const MASTER_KEY_BYTES: usize = 32;
const PROVIDER_STORE: KeyLabel = KeyLabel::new("vistoda-blink/provider-store/v1");

pub type SealingKey = Zeroizing<[u8; SUBKEY_BYTES]>;

/// Lazily loaded sub-key; the master key file is read (or created) once.
#[derive(Clone)]
pub struct StoreKey {
    path: PathBuf,
    key: Arc<OnceCell<SealingKey>>,
}

impl StoreKey {
    /// The key file next to the sealed store it protects.
    pub fn beside(store: &Path) -> Self {
        Self {
            path: store.with_file_name(STORE_KEY_FILE),
            key: Arc::default(),
        }
    }

    /// Key for opening an existing store; never creates key material, so a
    /// lost key file fails closed instead of silently replacing it.
    pub async fn existing(&self) -> Result<SealingKey, StoreError> {
        self.resolve(false).await
    }

    /// Key for sealing; creates the master key file on first use.
    pub async fn for_sealing(&self) -> Result<SealingKey, StoreError> {
        self.resolve(true).await
    }

    async fn resolve(&self, create: bool) -> Result<SealingKey, StoreError> {
        let path = self.path.clone();
        self.key
            .get_or_try_init(|| async move {
                tokio::task::spawn_blocking(move || load_or_create(&path, create))
                    .await
                    .map_err(|_| StoreError::Key("store key task failed".into()))?
            })
            .await
            .cloned()
    }
}

fn load_or_create(path: &Path, create: bool) -> Result<SealingKey, StoreError> {
    match read_private_file(path, MASTER_KEY_BYTES as u64) {
        Ok(material) => derive(&Zeroizing::new(material.into_inner())),
        Err(PrivateFileError::Io(error))
            if create && error.kind() == std::io::ErrorKind::NotFound =>
        {
            let mut material = Zeroizing::new([0_u8; MASTER_KEY_BYTES]);
            OsRng.fill_bytes(&mut *material);
            write_private_atomic(path, &*material)?;
            // Derive from what is on disk, so a failed write can never seal data
            // with a key that only existed in memory.
            let stored = read_private_file(path, MASTER_KEY_BYTES as u64)?;
            derive(&Zeroizing::new(stored.into_inner()))
        }
        Err(error) => Err(error.into()),
    }
}

fn derive(material: &[u8]) -> Result<SealingKey, StoreError> {
    if material.len() != MASTER_KEY_BYTES {
        return Err(StoreError::Key("store key must be exactly 32 bytes".into()));
    }
    let master = MasterKey::from_bytes(material.to_vec())
        .map_err(|error| StoreError::Key(error.to_string()))?;
    Ok(Zeroizing::new(
        *master.derive(PROVIDER_STORE).expose_secret(),
    ))
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::{STORE_KEY_FILE, StoreKey};
    use crate::credentials::StoreError;

    #[tokio::test]
    async fn key_file_is_created_private_once_and_reused() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let store = directory.path().join("provider.sealed");
        assert!(matches!(
            StoreKey::beside(&store).existing().await,
            Err(StoreError::PrivateFile(_))
        ));
        let first = StoreKey::beside(&store)
            .for_sealing()
            .await
            .expect("create key");
        let path = directory.path().join(STORE_KEY_FILE);
        let metadata = std::fs::metadata(&path).expect("key metadata");
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
        assert_eq!(metadata.len(), 32);
        let again = StoreKey::beside(&store)
            .existing()
            .await
            .expect("reuse key");
        assert_eq!(*first, *again);
    }

    #[tokio::test]
    async fn shared_or_malformed_key_files_are_rejected() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let store = directory.path().join("provider.sealed");
        let path = directory.path().join(STORE_KEY_FILE);
        std::fs::write(&path, [7_u8; 32]).expect("write key");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("chmod");
        assert!(matches!(
            StoreKey::beside(&store).for_sealing().await,
            Err(StoreError::PrivateFile(_))
        ));
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).expect("chmod");
        // A single repeated byte is placeholder material, not a key.
        assert!(matches!(
            StoreKey::beside(&store).for_sealing().await,
            Err(StoreError::Key(_))
        ));
        std::fs::write(&path, [7_u8; 16]).expect("write short key");
        assert!(matches!(
            StoreKey::beside(&store).for_sealing().await,
            Err(StoreError::Key(_))
        ));
    }
}
