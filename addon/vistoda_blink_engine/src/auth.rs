use axum::http::{HeaderMap, header};
use subtle::ConstantTimeEq;

use crate::error::EngineError;

pub fn require_bearer(headers: &HeaderMap, expected: &str) -> Result<(), EngineError> {
    let Some(value) = headers.get(header::AUTHORIZATION) else {
        return Err(EngineError::Unauthorized);
    };
    let Ok(value) = value.to_str() else {
        return Err(EngineError::Unauthorized);
    };
    let Some(presented) = value.strip_prefix("Bearer ") else {
        return Err(EngineError::Unauthorized);
    };
    if presented.as_bytes().ct_eq(expected.as_bytes()).into() {
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
