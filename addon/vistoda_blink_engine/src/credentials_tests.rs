#![allow(clippy::expect_used)]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use aes_gcm::{
    Aes256Gcm,
    aead::{Aead, AeadCore, KeyInit, OsRng},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use sha2::{Digest, Sha256};

use super::{CredentialStore, ProviderCredentials, StoreError};
use crate::store_key::STORE_KEY_FILE;

fn credentials() -> ProviderCredentials {
    ProviderCredentials {
        refresh_token: "sensitive-refresh".into(),
        hardware_id: "hardware".into(),
        region_id: Some("prod".into()),
        account_id: Some("42".into()),
        user_id: None,
        username: Some("user@example.com".into()),
    }
}

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path)
        .expect("metadata")
        .permissions()
        .mode()
        & 0o777
}

fn version(path: &Path) -> u64 {
    let raw = std::fs::read(path).expect("read envelope");
    let envelope: serde_json::Value = serde_json::from_slice(&raw).expect("envelope json");
    envelope["version"].as_u64().expect("version")
}

/// Writes a store exactly as releases before `store.key` did.
fn write_legacy(path: &Path, token: &str, credentials: &ProviderCredentials) {
    let mut digest = Sha256::new();
    digest.update(b"vistoda-blink-provider-store-v1\0");
    digest.update(token.as_bytes());
    let key: [u8; 32] = digest.finalize().into();
    let cipher = Aes256Gcm::new_from_slice(&key).expect("key");
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let plaintext = serde_json::to_vec(credentials).expect("encode");
    let ciphertext = cipher.encrypt(&nonce, plaintext.as_ref()).expect("seal");
    let envelope = serde_json::json!({
        "version": 1,
        "nonce": STANDARD.encode(nonce),
        "ciphertext": STANDARD.encode(ciphertext),
    });
    std::fs::write(path, serde_json::to_vec(&envelope).expect("json")).expect("write legacy");
}

#[tokio::test]
async fn round_trip_is_private_encrypted_and_bound_to_store_key() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("provider.sealed");
    let store = CredentialStore::new(path.clone(), &"a".repeat(64));
    store.save(&credentials()).await.expect("save credentials");
    let raw = tokio::fs::read_to_string(&path)
        .await
        .expect("read envelope");
    assert!(!raw.contains("sensitive-refresh"));
    assert_eq!(version(&path), 2);
    assert_eq!(mode(&path), 0o600);
    assert_eq!(mode(&directory.path().join(STORE_KEY_FILE)), 0o600);
    let loaded = store.load().await.expect("load").expect("present");
    assert_eq!(loaded.refresh_token, "sensitive-refresh");

    // The workload token no longer opens the store; only store.key does.
    let other_token = CredentialStore::new(path.clone(), &"b".repeat(64));
    let loaded = other_token.load().await.expect("load").expect("present");
    assert_eq!(loaded.refresh_token, "sensitive-refresh");
    std::fs::write(
        directory.path().join(STORE_KEY_FILE),
        (0_u8..32).collect::<Vec<_>>(),
    )
    .expect("replace key");
    let replaced = CredentialStore::new(path, &"a".repeat(64));
    assert!(matches!(
        replaced.load().await,
        Err(StoreError::Authentication)
    ));
}

#[tokio::test]
async fn a_missing_key_file_fails_closed_for_a_sealed_store() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("provider.sealed");
    CredentialStore::new(path.clone(), &"a".repeat(64))
        .save(&credentials())
        .await
        .expect("save credentials");
    std::fs::remove_file(directory.path().join(STORE_KEY_FILE)).expect("remove key");
    let store = CredentialStore::new(path, &"a".repeat(64));
    assert!(matches!(
        store.load().await,
        Err(StoreError::PrivateFile(_))
    ));
    assert!(!directory.path().join(STORE_KEY_FILE).exists());
}

#[tokio::test]
async fn a_legacy_store_is_migrated_to_store_key_on_first_load() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("provider.sealed");
    let token = "c".repeat(64);
    write_legacy(&path, &token, &credentials());
    assert!(!directory.path().join(STORE_KEY_FILE).exists());

    let store = CredentialStore::new(path.clone(), &token);
    let loaded = store.load().await.expect("load legacy").expect("present");
    assert_eq!(loaded.refresh_token, "sensitive-refresh");
    assert_eq!(version(&path), 2);
    assert_eq!(mode(&path), 0o600);
    assert_eq!(mode(&directory.path().join(STORE_KEY_FILE)), 0o600);

    // The re-sealed store opens without the token and the legacy key is gone.
    let fresh = CredentialStore::new(path.clone(), &"d".repeat(64));
    let reloaded = fresh.load().await.expect("load migrated").expect("present");
    assert_eq!(reloaded.hardware_id, "hardware");
    write_legacy(&path, &token, &credentials());
    let wrong_token = CredentialStore::new(path, &"d".repeat(64));
    assert!(matches!(
        wrong_token.load().await,
        Err(StoreError::Authentication)
    ));
}

#[tokio::test]
async fn unknown_envelope_versions_are_rejected() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("provider.sealed");
    std::fs::write(&path, br#"{"version":3,"nonce":"","ciphertext":""}"#).expect("write");
    let store = CredentialStore::new(path, &"a".repeat(64));
    assert!(matches!(store.load().await, Err(StoreError::Encoding)));
}
