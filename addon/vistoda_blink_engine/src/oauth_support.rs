use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngCore;
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};

const REDIRECT_URI: &str = "immedia-blink://applinks.blink.com/signin/callback";

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

#[cfg(test)]
mod tests {
    use super::extract_csrf;

    #[test]
    fn extracts_only_json_from_oauth_script() {
        let html = r#"<script id="oauth-args" type="application/json">
          {"csrf-token":"bounded-token","other":"value"}</script>"#;
        assert_eq!(extract_csrf(html).as_deref(), Some("bounded-token"));
        assert_eq!(extract_csrf("<script></script>"), None);
    }
}
