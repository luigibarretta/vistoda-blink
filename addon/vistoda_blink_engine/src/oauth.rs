use std::sync::Arc;

use reqwest::{Client, StatusCode, cookie::Jar};
use serde::Deserialize;
use thiserror::Error;
use url::Url;
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

pub use crate::oauth_support::{AccessToken, Endpoints};
use crate::oauth_support::{
    authorize_parameters, extract_csrf, oauth_client, pkce, refresh_revoked,
};

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

pub struct OAuthCompletion {
    pub tokens: TokenResponse,
    pub hardware_id: String,
}

pub enum SignInResult {
    Complete(OAuthCompletion),
    TwoFactor(PendingOAuth),
}

pub struct PendingOAuth {
    endpoints: Endpoints,
    client: Client,
    csrf: Zeroizing<String>,
    verifier: Zeroizing<String>,
    hardware_id: String,
}

impl PendingOAuth {
    pub async fn complete(self, code: &str) -> Result<OAuthCompletion, OAuthError> {
        let response = self
            .client
            .post(self.endpoints.verify_2fa())
            .header("User-Agent", USER_AGENT)
            .header("Origin", self.endpoints.origin())
            .header("Referer", self.endpoints.sign_in())
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
        let tokens = finish(
            &self.endpoints,
            &self.client,
            &self.verifier,
            &self.hardware_id,
        )
        .await?;
        Ok(OAuthCompletion {
            tokens,
            hardware_id: self.hardware_id,
        })
    }
}

pub async fn sign_in(username: &str, password: &str) -> Result<SignInResult, OAuthError> {
    sign_in_at(&Endpoints::production(), username, password).await
}

pub async fn sign_in_at(
    endpoints: &Endpoints,
    username: &str,
    password: &str,
) -> Result<SignInResult, OAuthError> {
    let cookies = Arc::new(Jar::default());
    let browser = oauth_client(Some(cookies.clone()), true)?;
    let client = oauth_client(Some(cookies), false)?;
    let hardware_id = Uuid::new_v4().hyphenated().to_string().to_uppercase();
    let (verifier, challenge) = pkce();
    let response = browser
        .get(endpoints.authorize())
        .header("User-Agent", USER_AGENT)
        .query(&authorize_parameters(&hardware_id, &challenge))
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(OAuthError::Unexpected);
    }
    let html = browser
        .get(endpoints.sign_in())
        .header("User-Agent", USER_AGENT)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let csrf = Zeroizing::new(extract_csrf(&html).ok_or(OAuthError::MissingCsrf)?);
    let response = client
        .post(endpoints.sign_in())
        .header("User-Agent", USER_AGENT)
        .header("Origin", endpoints.origin())
        .header("Referer", endpoints.sign_in())
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
            endpoints: endpoints.clone(),
            client,
            csrf,
            verifier,
            hardware_id,
        }));
    }
    if !response.status().is_redirection() {
        return Err(OAuthError::InvalidCredentials);
    }
    let tokens = finish(endpoints, &client, &verifier, &hardware_id).await?;
    Ok(SignInResult::Complete(OAuthCompletion {
        tokens,
        hardware_id,
    }))
}

pub async fn refresh(
    endpoints: &Endpoints,
    refresh_token: &str,
    hardware_id: &str,
) -> Result<TokenResponse, OAuthError> {
    let client = oauth_client(None, false)?;
    let response = client
        .post(endpoints.token())
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
    let status = response.status();
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::BAD_REQUEST {
        let body = response.bytes().await?;
        return Err(if refresh_revoked(status.as_u16(), &body) {
            OAuthError::InvalidCredentials
        } else {
            OAuthError::Unexpected
        });
    }
    Ok(response.error_for_status()?.json().await?)
}

async fn finish(
    endpoints: &Endpoints,
    client: &Client,
    verifier: &str,
    hardware_id: &str,
) -> Result<TokenResponse, OAuthError> {
    let response = client
        .get(endpoints.authorize())
        .header("User-Agent", USER_AGENT)
        .header("Referer", endpoints.sign_in())
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
        .post(endpoints.token())
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

#[cfg(test)]
#[path = "oauth_tests.rs"]
mod tests;
