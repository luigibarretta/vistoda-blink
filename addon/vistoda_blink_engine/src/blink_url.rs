//! Blink host validation for every request that carries the bearer token.

const BLINK_HOST_SUFFIX: &str = ".immedia-semi.com";

/// REST host for a Blink tier; `None` unless the tier matches `^[a-z0-9]+$`,
/// so a tampered tier can never move the bearer token to another host.
pub fn base_url(tier: &str) -> Option<String> {
    (!tier.is_empty()
        && tier.len() <= 32
        && tier
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit()))
    .then(|| format!("https://rest-{tier}.immedia-semi.com"))
}

/// Parse as the HTTP client does and accept only `https://*.immedia-semi.com`
/// without credentials, port or fragment, so the token never leaves Blink.
pub fn blink_https_url(value: &str) -> Option<String> {
    let url = reqwest::Url::parse(value).ok()?;
    let host = url.host_str()?;
    (url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.fragment().is_none()
        && host.ends_with(BLINK_HOST_SUFFIX))
    .then(|| url.to_string())
}

/// Resolve a media reference from Blink JSON: a path relative to the tier
/// host, or an absolute URL that must itself be a Blink HTTPS URL.
pub fn media_url(base: &str, value: &str) -> Option<String> {
    if value.starts_with('/') && !value.starts_with("//") {
        blink_https_url(&format!("{base}{value}"))
    } else {
        blink_https_url(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{base_url, media_url};

    #[test]
    fn tiers_are_lowercase_alphanumeric_only() {
        assert_eq!(
            base_url("e006").as_deref(),
            Some("https://rest-e006.immedia-semi.com")
        );
        assert!(base_url("prod").is_some());
        for tier in ["", "E006", "a.b", "evil.com#", "x/y", "a-b", "u011 ", "é"] {
            assert!(base_url(tier).is_none(), "{tier}");
        }
    }

    #[test]
    fn downloads_stay_on_blink_https_hosts() {
        let base = "https://rest-prod.immedia-semi.com";
        assert_eq!(
            media_url(base, "/api/v2/clip.mp4").as_deref(),
            Some("https://rest-prod.immedia-semi.com/api/v2/clip.mp4")
        );
        assert!(media_url(base, "https://media.immedia-semi.com/t.jpg?x=1").is_some());
        for value in [
            "//evil.com/x",
            "http://rest-prod.immedia-semi.com/x",
            "https://evil.com/x",
            "https://rest-prod.immedia-semi.com.evil.com/x",
            "https://evil.com\\.immedia-semi.com/x",
            "https://user@rest-prod.immedia-semi.com/x",
            "https://rest-prod.immedia-semi.com:8443/x",
            "https://evil.com#.immedia-semi.com",
            "ftp://rest-prod.immedia-semi.com/x",
            "relative/path",
        ] {
            assert!(media_url(base, value).is_none(), "{value}");
        }
    }
}
