use std::{
    error::Error,
    time::{Duration, Instant},
};

use super::{Endpoints, OAuthError, SignInResult, refresh, sign_in_at};
use crate::oauth_fake::{BAD_USER, FakeOAuth, TWO_FACTOR_CODE, TWO_FACTOR_USER, TokenReply};

type TestResult = Result<(), Box<dyn Error>>;

#[tokio::test]
async fn sign_in_without_two_factor_completes_the_pkce_exchange() -> TestResult {
    let fake = FakeOAuth::start(TokenReply::Rotate).await?;
    let SignInResult::Complete(done) =
        sign_in_at(&Endpoints::at(&fake.base), "user@example.com", "pw").await?
    else {
        return Err("expected a completed sign-in".into());
    };
    assert_eq!(done.tokens.access_token, "access-0");
    assert_eq!(done.tokens.refresh_token, "refresh-0");
    assert_eq!(done.hardware_id.len(), 36);
    Ok(())
}

#[tokio::test]
async fn two_factor_rejects_a_wrong_code_and_accepts_the_right_one() -> TestResult {
    let fake = FakeOAuth::start(TokenReply::Rotate).await?;
    let endpoints = Endpoints::at(&fake.base);
    let SignInResult::TwoFactor(pending) = sign_in_at(&endpoints, TWO_FACTOR_USER, "pw").await?
    else {
        return Err("expected a two-factor challenge".into());
    };
    assert!(matches!(
        pending.complete("000000").await,
        Err(OAuthError::InvalidTwoFactor)
    ));
    let SignInResult::TwoFactor(pending) = sign_in_at(&endpoints, TWO_FACTOR_USER, "pw").await?
    else {
        return Err("expected a two-factor challenge".into());
    };
    let done = pending.complete(TWO_FACTOR_CODE).await?;
    assert_eq!(done.tokens.refresh_token, "refresh-0");
    Ok(())
}

#[tokio::test]
async fn rejected_account_credentials_are_reported_as_such() -> TestResult {
    let fake = FakeOAuth::start(TokenReply::Rotate).await?;
    let result = sign_in_at(&Endpoints::at(&fake.base), BAD_USER, "pw").await;
    assert!(matches!(result, Err(OAuthError::InvalidCredentials)));
    Ok(())
}

async fn refresh_with(reply: TokenReply) -> Result<Result<String, OAuthError>, Box<dyn Error>> {
    let fake = FakeOAuth::start(reply).await?;
    let result = refresh(&Endpoints::at(&fake.base), "old-refresh", "HW").await;
    assert_eq!(
        fake.state
            .refresh_tokens
            .lock()
            .map_err(|_| "poisoned")?
            .as_slice(),
        ["old-refresh"]
    );
    Ok(result.map(|tokens| tokens.refresh_token.clone()))
}

#[tokio::test]
async fn refresh_maps_only_a_rejected_grant_to_invalid_credentials() -> TestResult {
    assert_eq!(refresh_with(TokenReply::Rotate).await??, "rotated-1");
    for reply in [
        TokenReply::Status(401, ""),
        TokenReply::Status(400, r#"{"error":"invalid_grant"}"#),
    ] {
        assert!(matches!(
            refresh_with(reply).await?,
            Err(OAuthError::InvalidCredentials)
        ));
    }
    assert!(matches!(
        refresh_with(TokenReply::Status(400, r#"{"error":"invalid_request"}"#)).await?,
        Err(OAuthError::Unexpected)
    ));
    for status in [403, 429, 503] {
        let result = refresh_with(TokenReply::Status(status, "")).await?;
        assert!(
            matches!(&result, Err(OAuthError::Transport(error))
                if error.status().map(|value| value.as_u16()) == Some(status)),
            "{status}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_stalled_token_endpoint_times_out() -> TestResult {
    let started = Instant::now();
    let result = refresh_with(TokenReply::Stall).await?;
    assert!(matches!(&result, Err(OAuthError::Transport(error)) if error.is_timeout()));
    assert!(started.elapsed() < Duration::from_secs(10));
    Ok(())
}
