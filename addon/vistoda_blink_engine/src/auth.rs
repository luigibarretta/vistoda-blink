use axum::http::HeaderMap;
use vistoda_provider_kit::auth::bearer_matches;

use crate::error::EngineError;

/// Constant-time bearer check (lib-vistoda-provider-kit `auth`).
pub fn require_bearer(headers: &HeaderMap, expected: &str) -> Result<(), EngineError> {
    if bearer_matches(headers, expected.as_bytes()) {
        Ok(())
    } else {
        Err(EngineError::Unauthorized)
    }
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue, header};

    use super::require_bearer;

    #[test]
    fn accepts_only_the_exact_bearer_token() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer secret"),
        );
        assert!(require_bearer(&headers, "secret").is_ok());
        assert!(require_bearer(&headers, "other").is_err());
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Basic secret"),
        );
        assert!(require_bearer(&headers, "secret").is_err());
    }
}
