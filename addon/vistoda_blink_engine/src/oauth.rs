use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

use reqwest::{Client, StatusCode, cookie::Jar, redirect::Policy};
use serde::Deserialize;
use thiserror::Error;
use url::Url;
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::oauth_support::{authorize_parameters, extract_csrf, pkce};

const BASE: &str = "https://api.oauth.blink.com";
const AUTHORIZE: &str = "https://api.oauth.blink.com/oauth/v2/authorize";
const SIGN_IN: &str = "https://api.oauth.blink.com/oauth/v2/signin";
const VERIFY_2FA: &str = "https://api.oauth.blink.com/oauth/v2/2fa/verify";
const TOKEN: &str = "https://api.oauth.blink.com/oauth/token";
const REDIRECT_URI: &str = "immedia-blink://applinks.blink.com/signin/callback";
const USER_AGENT: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_7 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.1 Mobile/15E148 Safari/604.1";
const TOKEN_USER_AGENT: &str = "Blink/2511191620 CFNetwork/3860.200.71 Darwin/25.1.0";

#[derive(Debug, Error)]
pub enum OAuthError {
    #[error("Blink OAuth transport failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("Blink rejected the account credentials")]
    InvalidCredentials,
    #[error("Blink OAuth returned an unexpected response")]
    Unexpected,
    #[error("Blink OAuth response did not contain a CSRF token")]
    MissingCsrf,
    #[error("Blink OAuth response did not contain an authorization code")]
    MissingCode,
    #[error("Blink rejected the two-factor code")]
    InvalidTwoFactor,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    #[zeroize(skip)]
    pub expires_in: u64,
}

pub struct AccessToken {
    pub value: Zeroizing<String>,
    pub expires_at: SystemTime,
}

impl From<&TokenResponse> for AccessToken {
    fn from(value: &TokenResponse) -> Self {
        Self {
            value: Zeroizing::new(value.access_token.clone()),
            expires_at: SystemTime::now() + Duration::from_secs(value.expires_in),
        }
    }
}

pub struct OAuthCompletion {
    pub tokens: TokenResponse,
    pub hardware_id: String,
}

pub enum SignInResult {
    Complete(OAuthCompletion),
    TwoFactor(PendingOAuth),
}

pub struct PendingOAuth {
    client: Client,
    csrf: Zeroizing<String>,
    verifier: Zeroizing<String>,
    hardware_id: String,
}

impl PendingOAuth {
    pub async fn complete(self, code: &str) -> Result<OAuthCompletion, OAuthError> {
        let response = self
            .client
            .post(VERIFY_2FA)
            .header("User-Agent", USER_AGENT)
            .header("Origin", BASE)
            .header("Referer", SIGN_IN)
            .form(&[
                ("2fa_code", code),
                ("csrf-token", self.csrf.as_str()),
                ("remember_me", "false"),
            ])
            .send()
            .await?;
        if response.status() != StatusCode::CREATED {
            return Err(OAuthError::InvalidTwoFactor);
        }
        let result: serde_json::Value = response.json().await?;
        if result.get("status").and_then(|value| value.as_str()) != Some("auth-completed") {
            return Err(OAuthError::InvalidTwoFactor);
        }
        let tokens = finish(&self.client, &self.verifier, &self.hardware_id).await?;
        Ok(OAuthCompletion {
            tokens,
            hardware_id: self.hardware_id,
        })
    }
}

pub async fn sign_in(username: &str, password: &str) -> Result<SignInResult, OAuthError> {
    let cookies = Arc::new(Jar::default());
    let browser = Client::builder().cookie_provider(cookies.clone()).build()?;
    let client = Client::builder()
        .cookie_provider(cookies)
        .redirect(Policy::none())
        .build()?;
    let hardware_id = Uuid::new_v4().hyphenated().to_string().to_uppercase();
    let (verifier, challenge) = pkce();
    let response = browser
        .get(AUTHORIZE)
        .header("User-Agent", USER_AGENT)
        .query(&authorize_parameters(&hardware_id, &challenge))
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(OAuthError::Unexpected);
    }
    let html = browser
        .get(SIGN_IN)
        .header("User-Agent", USER_AGENT)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let csrf = Zeroizing::new(extract_csrf(&html).ok_or(OAuthError::MissingCsrf)?);
    let response = client
        .post(SIGN_IN)
        .header("User-Agent", USER_AGENT)
        .header("Origin", BASE)
        .header("Referer", SIGN_IN)
        .form(&[
            ("username", username),
            ("password", password),
            ("csrf-token", csrf.as_str()),
        ])
        .send()
        .await?;
    if response.status() == StatusCode::PRECONDITION_FAILED
        || response.status() == StatusCode::ACCEPTED
    {
        return Ok(SignInResult::TwoFactor(PendingOAuth {
            client,
            csrf,
            verifier,
            hardware_id,
        }));
    }
    if !response.status().is_redirection() {
        return Err(OAuthError::InvalidCredentials);
    }
    let tokens = finish(&client, &verifier, &hardware_id).await?;
    Ok(SignInResult::Complete(OAuthCompletion {
        tokens,
        hardware_id,
    }))
}

pub async fn refresh(refresh_token: &str, hardware_id: &str) -> Result<TokenResponse, OAuthError> {
    let client = Client::builder().redirect(Policy::none()).build()?;
    let response = client
        .post(TOKEN)
        .header("User-Agent", TOKEN_USER_AGENT)
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", "ios"),
            ("scope", "client"),
            ("hardware_id", hardware_id),
        ])
        .send()
        .await?;
    if response.status() == StatusCode::UNAUTHORIZED {
        return Err(OAuthError::InvalidCredentials);
    }
    Ok(response.error_for_status()?.json().await?)
}

async fn finish(
    client: &Client,
    verifier: &str,
    hardware_id: &str,
) -> Result<TokenResponse, OAuthError> {
    let response = client
        .get(AUTHORIZE)
        .header("User-Agent", USER_AGENT)
        .header("Referer", SIGN_IN)
        .send()
        .await?;
    let location = response
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .ok_or(OAuthError::MissingCode)?;
    let code = Url::parse(location)
        .ok()
        .and_then(|url| {
            url.query_pairs()
                .find(|(key, _)| key == "code")
                .map(|(_, value)| value.into_owned())
        })
        .ok_or(OAuthError::MissingCode)?;
    let response = client
        .post(TOKEN)
        .header("User-Agent", TOKEN_USER_AGENT)
        .form(&[
            ("app_brand", "blink"),
            ("client_id", "ios"),
            ("code", code.as_str()),
            ("code_verifier", verifier),
            ("grant_type", "authorization_code"),
            ("hardware_id", hardware_id),
            ("redirect_uri", REDIRECT_URI),
            ("scope", "client"),
        ])
        .send()
        .await?;
    Ok(response.error_for_status()?.json().await?)
}
