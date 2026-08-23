use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

use reqwest::Client;
use tokio::sync::{Mutex, RwLock};
use zeroize::Zeroizing;

use crate::{
    blink_api::{self, TierInfo},
    blink_model::ProviderState,
    credentials::{CredentialStore, ProviderCredentials},
    oauth::{self, AccessToken, OAuthCompletion},
};

pub use crate::blink_error::BlinkError;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

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
    pub session: Mutex<Option<Session>>,
    pub state: RwLock<ProviderState>,
}

#[derive(Clone)]
pub struct BlinkClient {
    pub(crate) inner: Arc<Inner>,
}

impl BlinkClient {
    pub fn new(store: CredentialStore) -> Result<Self, BlinkError> {
        let http = Client::builder().timeout(REQUEST_TIMEOUT).build()?;
        Ok(Self {
            inner: Arc::new(Inner {
                http,
                store,
                session: Mutex::new(None),
                state: RwLock::new(ProviderState::default()),
            }),
        })
    }

    pub async fn bootstrap(&self) -> Result<bool, BlinkError> {
        let Some(credentials) = self.inner.store.load().await? else {
            return Ok(false);
        };
        *self.inner.session.lock().await = Some(Session {
            credentials,
            access: None,
        });
        self.refresh_state().await?;
        Ok(true)
    }

    pub async fn enroll(
        &self,
        completion: OAuthCompletion,
        username: Option<String>,
    ) -> Result<(), BlinkError> {
        let tier = self.tier_info(&completion.tokens.access_token).await?;
        let credentials = ProviderCredentials {
            refresh_token: completion.tokens.refresh_token.clone(),
            hardware_id: completion.hardware_id,
            region_id: Some(tier.tier),
            account_id: Some(tier.account_id.to_string()),
            user_id: None,
            username,
        };
        self.inner.store.save(&credentials).await?;
        *self.inner.session.lock().await = Some(Session {
            credentials,
            access: Some(AccessToken::from(&completion.tokens)),
        });
        self.refresh_state().await
    }

    pub async fn import(&self, credentials: ProviderCredentials) -> Result<(), BlinkError> {
        *self.inner.session.lock().await = Some(Session {
            credentials,
            access: None,
        });
        drop(self.context().await?);
        let credentials = {
            let session = self.inner.session.lock().await;
            session
                .as_ref()
                .ok_or(BlinkError::NotEnrolled)?
                .credentials
                .clone()
        };
        self.inner.store.save(&credentials).await?;
        self.refresh_state().await
    }

    pub async fn enrolled(&self) -> bool {
        self.inner.session.lock().await.is_some()
    }
    pub async fn state(&self) -> ProviderState {
        self.inner.state.read().await.clone()
    }

    pub(crate) async fn context(&self) -> Result<RequestContext, BlinkError> {
        let mut guard = self.inner.session.lock().await;
        let session = guard.as_mut().ok_or(BlinkError::NotEnrolled)?;
        let refresh_needed = session.access.as_ref().is_none_or(|access| {
            access
                .expires_at
                .duration_since(SystemTime::now())
                .unwrap_or_default()
                < Duration::from_secs(60)
        });
        if refresh_needed {
            let tokens = oauth::refresh(
                &session.credentials.refresh_token,
                &session.credentials.hardware_id,
            )
            .await
            .map_err(|_| BlinkError::Authentication)?;
            session
                .credentials
                .refresh_token
                .clone_from(&tokens.refresh_token);
            session.access = Some(AccessToken::from(&tokens));
            self.inner.store.save(&session.credentials).await?;
        }
        let result = RequestContext {
            token: session
                .access
                .as_ref()
                .ok_or(BlinkError::Authentication)?
                .value
                .clone(),
            base_url: blink_api::base_url(
                session
                    .credentials
                    .region_id
                    .as_deref()
                    .ok_or(BlinkError::InvalidResponse)?,
            ),
            account_id: session
                .credentials
                .account_id
                .clone()
                .ok_or(BlinkError::InvalidResponse)?,
        };
        drop(guard);
        Ok(result)
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
