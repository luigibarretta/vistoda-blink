//! Refresh-and-rotate against the in-process fake OAuth server.

use std::{error::Error, path::Path, time::Duration};

use crate::{
    alias_store::AliasStore,
    blink_client::{BlinkClient, BlinkError, Session},
    credentials::{CredentialStore, ProviderCredentials},
    oauth::Endpoints,
    oauth_fake::{FakeOAuth, TokenReply},
};

type TestResult = Result<(), Box<dyn Error>>;

fn store(path: &Path) -> CredentialStore {
    CredentialStore::new(path.to_path_buf(), &"a".repeat(64))
}

async fn enrolled(fake: &FakeOAuth, sealed: &Path) -> Result<BlinkClient, Box<dyn Error>> {
    let aliases = sealed.with_file_name("camera-aliases.json");
    let client = BlinkClient::with_oauth(
        store(sealed),
        AliasStore::new(aliases),
        Endpoints::at(&fake.base),
    )?;
    let guard = client.credential_lock().await;
    client
        .replace_session(Session {
            credentials: ProviderCredentials {
                refresh_token: "seed-refresh".into(),
                hardware_id: "HW".into(),
                region_id: Some("prod".into()),
                account_id: Some("1".into()),
                user_id: None,
                username: None,
            },
            access: None,
        })
        .await;
    drop(guard);
    Ok(client)
}

async fn memory_refresh(client: &BlinkClient) -> Option<String> {
    let session = client.inner.session.lock().await;
    session
        .as_ref()
        .map(|value| value.credentials.refresh_token.clone())
}

async fn disk_refresh(path: &Path) -> Result<Option<String>, Box<dyn Error>> {
    Ok(store(path)
        .load()
        .await?
        .map(|value| value.refresh_token.clone()))
}

#[tokio::test]
async fn refresh_rotates_memory_and_disk_once_for_concurrent_callers() -> TestResult {
    let fake = FakeOAuth::start(TokenReply::Rotate).await?;
    fake.set_delay(Duration::from_millis(200));
    let directory = tempfile::tempdir()?;
    let sealed = directory.path().join("provider.sealed");
    let client = enrolled(&fake, &sealed).await?;
    let calls = (0..8).map(|_| client.context());
    for context in futures_util::future::join_all(calls).await {
        let context = context?;
        assert_eq!(context.token.as_str(), "access-1");
        assert_eq!(context.base_url, "https://rest-prod.immedia-semi.com");
    }
    assert_eq!(fake.calls(), 1);
    assert_eq!(memory_refresh(&client).await.as_deref(), Some("rotated-1"));
    assert_eq!(disk_refresh(&sealed).await?.as_deref(), Some("rotated-1"));
    let health = client.session_health();
    assert_eq!(health.refresh_success, 1);
    assert!(!health.persist_pending && !health.reauth_required);
    Ok(())
}

#[tokio::test]
async fn a_cancelled_request_still_persists_the_rotated_token() -> TestResult {
    let fake = FakeOAuth::start(TokenReply::Rotate).await?;
    fake.set_delay(Duration::from_millis(300));
    let directory = tempfile::tempdir()?;
    let sealed = directory.path().join("provider.sealed");
    let client = enrolled(&fake, &sealed).await?;
    let caller = client.clone();
    let request = tokio::spawn(async move { caller.context().await.map(drop) });
    tokio::time::sleep(Duration::from_millis(100)).await;
    request.abort();
    for _ in 0..40 {
        if disk_refresh(&sealed).await?.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(disk_refresh(&sealed).await?.as_deref(), Some("rotated-1"));
    assert_eq!(memory_refresh(&client).await.as_deref(), Some("rotated-1"));
    client.context().await?;
    assert_eq!(fake.calls(), 1);
    Ok(())
}

#[tokio::test]
async fn a_failed_write_keeps_the_token_in_memory_and_retries() -> TestResult {
    let fake = FakeOAuth::start(TokenReply::Rotate).await?;
    let directory = tempfile::tempdir()?;
    let missing = directory.path().join("missing");
    let sealed = missing.join("provider.sealed");
    let client = enrolled(&fake, &sealed).await?;
    assert_eq!(client.context().await?.token.as_str(), "access-1");
    let health = client.session_health();
    assert!(health.persist_pending && health.persist_failures >= 1);
    assert_eq!(memory_refresh(&client).await.as_deref(), Some("rotated-1"));
    std::fs::create_dir(&missing)?;
    for _ in 0..100 {
        if !client.session_health().persist_pending {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(!client.session_health().persist_pending);
    assert_eq!(disk_refresh(&sealed).await?.as_deref(), Some("rotated-1"));
    Ok(())
}

#[tokio::test]
async fn a_rejected_grant_latches_reauth_without_hammering_blink() -> TestResult {
    let fake = FakeOAuth::start(TokenReply::Status(401, "")).await?;
    let directory = tempfile::tempdir()?;
    let client = enrolled(&fake, &directory.path().join("provider.sealed")).await?;
    assert!(matches!(
        client.context().await,
        Err(BlinkError::Authentication)
    ));
    assert!(client.session_health().reauth_required);
    assert!(matches!(
        client.context().await,
        Err(BlinkError::Authentication)
    ));
    assert_eq!(fake.calls(), 1);
    assert_eq!(
        memory_refresh(&client).await.as_deref(),
        Some("seed-refresh")
    );
    Ok(())
}

#[tokio::test]
async fn a_transient_failure_stays_retryable_and_cools_down() -> TestResult {
    let fake = FakeOAuth::start(TokenReply::Status(503, "")).await?;
    let directory = tempfile::tempdir()?;
    let client = enrolled(&fake, &directory.path().join("provider.sealed")).await?;
    assert!(matches!(client.context().await, Err(BlinkError::OAuth(_))));
    assert!(matches!(client.context().await, Err(BlinkError::OAuth(_))));
    assert_eq!(fake.calls(), 1);
    let health = client.session_health();
    assert!(!health.reauth_required);
    assert_eq!(health.refresh_failure, 1);
    Ok(())
}
