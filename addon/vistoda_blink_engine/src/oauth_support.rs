use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngCore;
use reqwest::{Client, cookie::Jar, redirect::Policy};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};

const REDIRECT_URI: &str = "immedia-blink://applinks.blink.com/signin/callback";
const PRODUCTION_BASE: &str = "https://api.oauth.blink.com";
/// Every OAuth request ends within this bound, so a stalled token endpoint
/// cannot hold a refresh (and every caller waiting for it) indefinitely.
pub const OAUTH_TIMEOUT: Duration = if cfg!(test) {
    Duration::from_secs(2)
} else {
    Duration::from_secs(20)
};
const OAUTH_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

pub struct AccessToken {
    pub value: Zeroizing<String>,
    pub expires_at: SystemTime,
}

impl From<&crate::oauth::TokenResponse> for AccessToken {
    fn from(value: &crate::oauth::TokenResponse) -> Self {
        Self {
            value: Zeroizing::new(value.access_token.clone()),
            expires_at: SystemTime::now() + Duration::from_secs(value.expires_in),
        }
    }
}

/// Blink OAuth endpoint set; tests point it at an in-process fake server.
#[derive(Clone, Debug)]
pub struct Endpoints {
    base: String,
}

impl Endpoints {
    pub fn production() -> Self {
        Self::at(PRODUCTION_BASE)
    }

    pub fn at(base: &str) -> Self {
        Self {
            base: base.trim_end_matches('/').to_owned(),
        }
    }

    pub fn origin(&self) -> &str {
        &self.base
    }

    pub fn authorize(&self) -> String {
        format!("{}/oauth/v2/authorize", self.base)
    }

    pub fn sign_in(&self) -> String {
        format!("{}/oauth/v2/signin", self.base)
    }

    pub fn verify_2fa(&self) -> String {
        format!("{}/oauth/v2/2fa/verify", self.base)
    }

    pub fn token(&self) -> String {
        format!("{}/oauth/token", self.base)
    }
}

/// A bounded OAuth HTTP client; `follow` keeps the browser-style redirects.
pub fn oauth_client(cookies: Option<Arc<Jar>>, follow: bool) -> reqwest::Result<Client> {
    let mut builder = Client::builder()
        .connect_timeout(OAUTH_CONNECT_TIMEOUT)
        .timeout(OAUTH_TIMEOUT);
    if let Some(cookies) = cookies {
        builder = builder.cookie_provider(cookies);
    }
    if !follow {
        builder = builder.redirect(Policy::none());
    }
    builder.build()
}

pub fn authorize_parameters(hardware_id: &str, challenge: &str) -> Vec<(&'static str, String)> {
    [
        ("app_brand", "blink"),
        ("app_version", "50.1"),
        ("client_id", "ios"),
        ("code_challenge", challenge),
        ("code_challenge_method", "S256"),
        ("device_brand", "Apple"),
        ("device_model", "iPhone16,1"),
        ("device_os_version", "26.1"),
        ("hardware_id", hardware_id),
        ("redirect_uri", REDIRECT_URI),
        ("response_type", "code"),
        ("scope", "client"),
    ]
    .into_iter()
    .map(|(key, value)| (key, value.to_owned()))
    .collect()
}

pub fn pkce() -> (Zeroizing<String>, String) {
    let mut random = [0_u8; 32];
    rand::rng().fill_bytes(&mut random);
    let verifier = Zeroizing::new(URL_SAFE_NO_PAD.encode(random));
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    random.zeroize();
    (verifier, challenge)
}

pub fn extract_csrf(html: &str) -> Option<String> {
    let marker_offset = html
        .find("id=\"oauth-args\"")
        .or_else(|| html.find("id='oauth-args'"))?;
    let body_start = html[marker_offset..].find('>')? + marker_offset + 1;
    let body_end = html[body_start..].find("</script>")? + body_start;
    let value: serde_json::Value = serde_json::from_str(html[body_start..body_end].trim()).ok()?;
    value.get("csrf-token")?.as_str().map(ToOwned::to_owned)
}

/// True when Blink's token endpoint has permanently rejected the refresh
/// grant: HTTP 401, or the OAuth 2.0 `invalid_grant` error on HTTP 400.
/// Transport failures, rate limits and 5xx stay retryable, so a network
/// outage never forces the user through a new sign-in.
pub fn refresh_revoked(status: u16, body: &[u8]) -> bool {
    match status {
        401 => true,
        400 => serde_json::from_slice::<serde_json::Value>(body)
            .ok()
            .and_then(|value| value.get("error")?.as_str().map(ToOwned::to_owned))
            .is_some_and(|error| error == "invalid_grant"),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{extract_csrf, refresh_revoked};

    #[test]
    fn only_a_rejected_grant_requires_a_new_sign_in() {
        assert!(refresh_revoked(401, b""));
        assert!(refresh_revoked(400, br#"{"error":"invalid_grant"}"#));
        assert!(!refresh_revoked(400, br#"{"error":"invalid_request"}"#));
        assert!(!refresh_revoked(400, b"<html>"));
        assert!(!refresh_revoked(403, br#"{"error":"invalid_grant"}"#));
        assert!(!refresh_revoked(429, b""));
        assert!(!refresh_revoked(503, b""));
    }

    #[test]
    fn extracts_only_json_from_oauth_script() {
        let html = r#"<script id="oauth-args" type="application/json">
          {"csrf-token":"bounded-token","other":"value"}</script>"#;
        assert_eq!(extract_csrf(html).as_deref(), Some("bounded-token"));
        assert_eq!(extract_csrf("<script></script>"), None);
    }
}
