//! Single-flight OAuth refresh and persistence of the rotated refresh token.
//!
//! Blink rotates the refresh token on every refresh, so two refreshes with
//! the same token would race. The session mutex is held only to read or write
//! memory, never across a network call. `refresh_lock` serializes refreshes
//! and every credential-file write. The refresh itself runs in a detached
//! task that owns the lock, so a cancelled HTTP request cannot stop it between
//! Blink rotating the token and the new token reaching memory and disk.

use std::{
    sync::{Arc, atomic::Ordering},
    time::{Duration, SystemTime},
};

use tokio::sync::OwnedMutexGuard;
use zeroize::Zeroizing;

use crate::{
    blink_api,
    blink_client::{BlinkClient, BlinkError, RequestContext, Session},
    credentials::StoreError,
    oauth::{self, AccessToken, OAuthError},
};

const REFRESH_MARGIN: Duration = Duration::from_secs(60);
const PERSIST_RETRY_BASE: Duration = if cfg!(test) {
    Duration::from_millis(50)
} else {
    Duration::from_secs(2)
};
const PERSIST_RETRY_MAX: Duration = Duration::from_secs(300);

fn needs_refresh(session: &Session) -> bool {
    session.access.as_ref().is_none_or(|access| {
        access
            .expires_at
            .duration_since(SystemTime::now())
            .unwrap_or_default()
            < REFRESH_MARGIN
    })
}

fn request_context(session: &Session) -> Result<RequestContext, BlinkError> {
    let region = session
        .credentials
        .region_id
        .as_deref()
        .ok_or(BlinkError::InvalidResponse)?;
    Ok(RequestContext {
        token: session
            .access
            .as_ref()
            .ok_or(BlinkError::Authentication)?
            .value
            .clone(),
        base_url: blink_api::base_url(region).ok_or(BlinkError::InvalidResponse)?,
        account_id: session
            .credentials
            .account_id
            .clone()
            .ok_or(BlinkError::InvalidResponse)?,
    })
}

impl BlinkClient {
    pub(crate) async fn context(&self) -> Result<RequestContext, BlinkError> {
        {
            let guard = self.inner.session.lock().await;
            let session = guard.as_ref().ok_or(BlinkError::NotEnrolled)?;
            if !needs_refresh(session) {
                return request_context(session);
            }
        }
        self.refresh_access().await?;
        let guard = self.inner.session.lock().await;
        request_context(guard.as_ref().ok_or(BlinkError::NotEnrolled)?)
    }

    /// Replaces the in-memory session; callers hold `refresh_lock`.
    pub(crate) async fn replace_session(&self, session: Session) {
        *self.inner.session.lock().await = Some(session);
        self.inner.generation.fetch_add(1, Ordering::SeqCst);
        self.inner.health.credentials_replaced();
    }

    pub(crate) async fn credential_lock(&self) -> OwnedMutexGuard<()> {
        Arc::clone(&self.inner.refresh_lock).lock_owned().await
    }

    async fn refresh_access(&self) -> Result<(), BlinkError> {
        let guard = self.credential_lock().await;
        let (refresh_token, hardware_id, generation) = {
            let session = self.inner.session.lock().await;
            let session = session.as_ref().ok_or(BlinkError::NotEnrolled)?;
            if !needs_refresh(session) {
                return Ok(());
            }
            (
                Zeroizing::new(session.credentials.refresh_token.clone()),
                session.credentials.hardware_id.clone(),
                self.inner.generation.load(Ordering::SeqCst),
            )
        };
        if let Some(error) = self.inner.health.recent_failure(generation) {
            return Err(error);
        }
        let client = self.clone();
        tokio::spawn(async move {
            let _guard = guard;
            client
                .rotate(&refresh_token, &hardware_id, generation)
                .await
        })
        .await
        .map_err(|_| BlinkError::OAuth(OAuthError::Unexpected))?
    }

    async fn rotate(
        &self,
        refresh_token: &str,
        hardware_id: &str,
        generation: u64,
    ) -> Result<(), BlinkError> {
        let tokens = match oauth::refresh(&self.inner.oauth, refresh_token, hardware_id).await {
            Ok(tokens) => tokens,
            Err(error) => {
                let error = BlinkError::from_refresh(error);
                tracing::warn!(error = %error.redacted(), "Blink OAuth refresh failed");
                self.inner.health.refresh_failed(generation, &error);
                return Err(error);
            }
        };
        {
            let mut session = self.inner.session.lock().await;
            let Some(session) = session
                .as_mut()
                .filter(|_| self.inner.generation.load(Ordering::SeqCst) == generation)
            else {
                return Ok(());
            };
            session
                .credentials
                .refresh_token
                .clone_from(&tokens.refresh_token);
            session.access = Some(AccessToken::from(&tokens));
        }
        self.inner.health.refresh_succeeded();
        // Memory already holds the rotated token: the old one is useless, so
        // a failed write is retried in the background instead of failing.
        let _ = self.persist_locked().await;
        Ok(())
    }

    /// Writes the current credentials; callers hold `refresh_lock`. A failure
    /// flags the disagreement and starts the single background retry.
    pub(crate) async fn persist_locked(&self) -> Result<(), StoreError> {
        let result = self.save_current().await;
        if result.is_err() && self.inner.health.start_retry() {
            self.spawn_persist_retry();
        }
        result
    }

    async fn save_current(&self) -> Result<(), StoreError> {
        let Some(credentials) = self
            .inner
            .session
            .lock()
            .await
            .as_ref()
            .map(|session| session.credentials.clone())
        else {
            return Ok(());
        };
        match self.inner.store.save(&credentials).await {
            Ok(()) => {
                if self.inner.health.persisted() {
                    tracing::info!("Blink credential file is in sync with memory again");
                }
                Ok(())
            }
            Err(error) => {
                tracing::error!(%error,
                    "Blink credentials could not be saved; keeping them in memory and retrying");
                self.inner.health.persist_failed();
                Err(error)
            }
        }
    }

    fn spawn_persist_retry(&self) {
        let client = self.clone();
        tokio::spawn(async move {
            let mut delay = PERSIST_RETRY_BASE;
            loop {
                tokio::time::sleep(delay).await;
                let _guard = client.credential_lock().await;
                if !client.inner.health.persist_pending() || client.save_current().await.is_ok() {
                    client.inner.health.stop_retry();
                    break;
                }
                delay = (delay * 2).min(PERSIST_RETRY_MAX);
            }
        });
    }
}

#[cfg(test)]
#[path = "blink_session_tests.rs"]
mod tests;
