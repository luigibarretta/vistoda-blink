use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use reqwest::{
    Client,
    header::{ACCEPT, ACCEPT_LANGUAGE, HeaderMap, HeaderName, HeaderValue, USER_AGENT},
};
use tokio::sync::{Mutex, RwLock};
use zeroize::Zeroizing;

use crate::{
    alias_store::AliasStore,
    blink_api::{self, TierInfo},
    blink_model::ProviderState,
    blink_session_health::{SessionHealth, SessionHealthSnapshot},
    credentials::{CredentialStore, ProviderCredentials},
    oauth::{AccessToken, Endpoints, OAuthCompletion},
};

pub use crate::blink_error::BlinkError;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const BLINK_APP_BUILD: &str = "ANDROID_29715642";
const BLINK_USER_AGENT: &str = "Blink/57.1 (samsung SM-G998B; Android 14)";

pub(crate) struct Session {
    pub credentials: ProviderCredentials,
    pub access: Option<AccessToken>,
}

pub(crate) struct RequestContext {
    pub token: Zeroizing<String>,
    pub base_url: String,
    pub account_id: String,
}

pub(crate) struct Inner {
    pub http: Client,
    pub store: CredentialStore,
    pub aliases: AliasStore,
    pub session: Mutex<Option<Session>>,
    /// Serializes OAuth refreshes and credential-file writes (`blink_session`).
    pub refresh_lock: Arc<Mutex<()>>,
    /// Bumped whenever enrollment, import or startup replaces the session.
    pub generation: AtomicU64,
    pub health: SessionHealth,
    pub oauth: Endpoints,
    pub settings_lock: Mutex<()>,
    pub storage_lock: Mutex<()>,
    pub state: RwLock<ProviderState>,
    /// Last program-list read; programs refresh slower than the state.
    pub programs_read_at: Mutex<Option<Instant>>,
}

#[derive(Clone)]
pub struct BlinkClient {
    pub(crate) inner: Arc<Inner>,
}

impl BlinkClient {
    pub fn new(store: CredentialStore, aliases: AliasStore) -> Result<Self, BlinkError> {
        Self::with_oauth(store, aliases, Endpoints::production())
    }

    pub(crate) fn with_oauth(
        store: CredentialStore,
        aliases: AliasStore,
        oauth: Endpoints,
    ) -> Result<Self, BlinkError> {
        let http = Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .default_headers(provider_headers())
            .build()?;
        Ok(Self {
            inner: Arc::new(Inner {
                http,
                store,
                aliases,
                session: Mutex::new(None),
                refresh_lock: Arc::new(Mutex::new(())),
                generation: AtomicU64::new(0),
                health: SessionHealth::default(),
                oauth,
                settings_lock: Mutex::new(()),
                storage_lock: Mutex::new(()),
                state: RwLock::new(ProviderState::default()),
                programs_read_at: Mutex::new(None),
            }),
        })
    }

    pub async fn bootstrap(&self) -> Result<bool, BlinkError> {
        let Some(credentials) = self.inner.store.load().await? else {
            return Ok(false);
        };
        let guard = self.credential_lock().await;
        self.replace_session(Session {
            credentials,
            access: None,
        })
        .await;
        drop(guard);
        self.refresh_state().await?;
        Ok(true)
    }

    pub async fn enroll(
        &self,
        completion: OAuthCompletion,
        username: Option<String>,
    ) -> Result<(), BlinkError> {
        let tier = self.tier_info(&completion.tokens.access_token).await?;
        // The tier becomes a host name: reject anything but `[a-z0-9]+`.
        blink_api::base_url(&tier.tier).ok_or(BlinkError::InvalidResponse)?;
        let credentials = ProviderCredentials {
            refresh_token: completion.tokens.refresh_token.clone(),
            hardware_id: completion.hardware_id,
            region_id: Some(tier.tier),
            account_id: Some(tier.account_id.to_string()),
            user_id: None,
            username,
        };
        let guard = self.credential_lock().await;
        self.inner.store.save(&credentials).await?;
        self.replace_session(Session {
            credentials,
            access: Some(AccessToken::from(&completion.tokens)),
        })
        .await;
        self.inner.health.persisted();
        drop(guard);
        self.refresh_state().await
    }

    /// The refresh inside `context` rotates and saves the imported token; the
    /// explicit write below makes a failed save visible to the importer. An
    /// import Blink refuses restores the previous session.
    pub async fn import(&self, credentials: ProviderCredentials) -> Result<(), BlinkError> {
        // Validate everything `context` needs before the refresh: a failure
        // after Blink rotated the token would otherwise burn the import.
        let region = credentials.region_id.as_deref().unwrap_or_default();
        if blink_api::base_url(region).is_none()
            || credentials.account_id.as_deref().is_none_or(str::is_empty)
            || credentials.refresh_token.is_empty()
            || credentials.hardware_id.is_empty()
        {
            return Err(BlinkError::InvalidResponse);
        }
        let guard = self.credential_lock().await;
        let previous = self.inner.session.lock().await.take();
        self.replace_session(Session {
            credentials,
            access: None,
        })
        .await;
        let generation = self.inner.generation.load(Ordering::SeqCst);
        drop(guard);
        if let Err(error) = self.context().await {
            let _guard = self.credential_lock().await;
            if self.inner.generation.load(Ordering::SeqCst) == generation {
                if let Some(previous) = previous {
                    self.replace_session(previous).await;
                } else {
                    *self.inner.session.lock().await = None;
                }
            }
            return Err(error);
        }
        let guard = self.credential_lock().await;
        self.persist_locked().await?;
        drop(guard);
        self.refresh_state().await
    }

    pub async fn enrolled(&self) -> bool {
        self.inner.session.lock().await.is_some()
    }
    pub async fn state(&self) -> ProviderState {
        self.inner.state.read().await.clone()
    }
    pub fn session_health(&self) -> SessionHealthSnapshot {
        self.inner.health.snapshot()
    }

    async fn tier_info(&self, token: &str) -> Result<TierInfo, BlinkError> {
        Ok(self
            .inner
            .http
            .get(blink_api::TIER_URL)
            .bearer_auth(token)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }
}

fn provider_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-US,en;q=0.9"));
    headers.insert(USER_AGENT, HeaderValue::from_static(BLINK_USER_AGENT));
    headers.insert(
        HeaderName::from_static("app-build"),
        HeaderValue::from_static(BLINK_APP_BUILD),
    );
    headers.insert(
        HeaderName::from_static("locale"),
        HeaderValue::from_static("en_US"),
    );
    headers.insert(
        HeaderName::from_static("x-blink-time-zone"),
        HeaderValue::from_static("UTC"),
    );
    headers
}

#[cfg(test)]
mod tests {
    use super::provider_headers;

    #[test]
    fn provider_requests_match_the_native_rest_header_contract() {
        let headers = provider_headers();
        assert_eq!(headers["app-build"], "ANDROID_29715642");
        assert_eq!(headers["locale"], "en_US");
        assert_eq!(headers["x-blink-time-zone"], "UTC");
        assert_eq!(headers["accept"], "application/json");
    }
}
