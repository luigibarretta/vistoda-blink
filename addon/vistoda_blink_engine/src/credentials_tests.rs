#![allow(clippy::expect_used)]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use vistoda_provider_kit::{
    private_file::FileError,
    sealed_store::{KEY_FILE_NAME, SealError, SealingKey, VERSION_LEGACY_KEY, seal},
};

use super::{CredentialStore, LEGACY_STORE_LABEL, ProviderCredentials, StoreError};

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

fn write_private(path: &Path, bytes: &[u8]) {
    std::fs::write(path, bytes).expect("write");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).expect("chmod");
}

/// Writes a store exactly as releases before `store.key` did.
fn write_legacy(path: &Path, token: &str, credentials: &ProviderCredentials) {
    let key = SealingKey::from_label_and_material(LEGACY_STORE_LABEL, token.as_bytes());
    let plaintext = serde_json::to_vec(credentials).expect("encode");
    write_private(
        path,
        &seal(&key, VERSION_LEGACY_KEY, &plaintext).expect("seal"),
    );
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
    assert_eq!(mode(&directory.path().join(KEY_FILE_NAME)), 0o600);
    let loaded = store.load().await.expect("load").expect("present");
    assert_eq!(loaded.refresh_token, "sensitive-refresh");

    // The workload token no longer opens the store; only store.key does.
    let other_token = CredentialStore::new(path.clone(), &"b".repeat(64));
    let loaded = other_token.load().await.expect("load").expect("present");
    assert_eq!(loaded.refresh_token, "sensitive-refresh");
    write_private(
        &directory.path().join(KEY_FILE_NAME),
        &(0_u8..32).collect::<Vec<_>>(),
    );
    let replaced = CredentialStore::new(path, &"a".repeat(64));
    assert!(matches!(
        replaced.load().await,
        Err(StoreError::Sealed(SealError::Authentication))
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
    std::fs::remove_file(directory.path().join(KEY_FILE_NAME)).expect("remove key");
    let store = CredentialStore::new(path, &"a".repeat(64));
    assert!(matches!(
        store.load().await,
        Err(StoreError::Sealed(SealError::MissingKey(_)))
    ));
    assert!(!directory.path().join(KEY_FILE_NAME).exists());
}

#[tokio::test]
async fn a_legacy_store_is_migrated_to_store_key_on_first_load() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("provider.sealed");
    let token = "c".repeat(64);
    write_legacy(&path, &token, &credentials());
    assert!(!directory.path().join(KEY_FILE_NAME).exists());

    let store = CredentialStore::new(path.clone(), &token);
    let loaded = store.load().await.expect("load legacy").expect("present");
    assert_eq!(loaded.refresh_token, "sensitive-refresh");
    assert_eq!(version(&path), 2);
    assert_eq!(mode(&path), 0o600);
    assert_eq!(mode(&directory.path().join(KEY_FILE_NAME)), 0o600);

    // The re-sealed store opens without the token and the legacy key is gone.
    let fresh = CredentialStore::new(path.clone(), &"d".repeat(64));
    let reloaded = fresh.load().await.expect("load migrated").expect("present");
    assert_eq!(reloaded.hardware_id, "hardware");
    write_legacy(&path, &token, &credentials());
    let wrong_token = CredentialStore::new(path, &"d".repeat(64));
    assert!(matches!(
        wrong_token.load().await,
        Err(StoreError::Sealed(SealError::Authentication))
    ));
}

#[tokio::test]
async fn unknown_envelope_versions_and_shared_files_are_rejected() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("provider.sealed");
    write_private(&path, br#"{"version":3,"nonce":"","ciphertext":""}"#);
    let store = CredentialStore::new(path.clone(), &"a".repeat(64));
    assert!(matches!(
        store.load().await,
        Err(StoreError::Sealed(SealError::UnsupportedVersion(3)))
    ));
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("chmod");
    assert!(matches!(
        store.load().await,
        Err(StoreError::Sealed(SealError::File(FileError::Private(_))))
    ));
}

/// Stores written by the engine before it used the shared kit (0.25.1, the
/// fixtures of the vendored kit crate) keep opening.
#[tokio::test]
async fn stores_written_before_the_shared_kit_still_open() {
    let fixtures =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("vendor/vistoda-provider-kit/tests/fixtures");
    let expected: serde_json::Value = serde_json::from_slice(
        &std::fs::read(fixtures.join("blink-provider.json")).expect("plain"),
    )
    .expect("json");
    for (fixture, token) in [
        ("blink-provider-v2.sealed", "z"),
        ("blink-provider-v1.sealed", "c"),
    ] {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("provider.sealed");
        write_private(
            &path,
            &std::fs::read(fixtures.join(fixture)).expect("fixture"),
        );
        let key = (0_u8..32).collect::<Vec<_>>();
        write_private(&directory.path().join(KEY_FILE_NAME), &key);
        let store = CredentialStore::new(path, &token.repeat(64));
        let loaded = store.load().await.expect("load").expect("present");
        assert_eq!(
            loaded.refresh_token,
            expected["refresh_token"].as_str().expect("token")
        );
        assert_eq!(loaded.username.as_deref(), expected["username"].as_str());
    }
}
