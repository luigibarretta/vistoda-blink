//! Blink v4 motion poller: 15 s (±3 s) while armed, 30 s for 30 minutes after
//! a Blink rate limit, five minutes while disarmed (ADR 0012, ADR 0015).
//!
//! Failures back off to at most two minutes while armed, so motion resumes
//! quickly after a transient outage; rate limits and disarmed back off to 15.

use std::time::Duration;

use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{blink_client::BlinkError, hub::EngineState, motion::PollerStatus};

const FAST_ARMED_MS: u64 = 15_000;
pub const JITTER_MS: i64 = 3_000;
/// Hard floor: jitter or a future tweak never polls Blink faster than this.
const MIN_ARMED_MS: u64 = 12_000;
/// The 0.20–0.22 armed interval, restored after Blink HTTP 429/403.
const CONSERVATIVE_ARMED: Duration = Duration::from_secs(30);
const DISARMED: Duration = Duration::from_secs(300);
const MAX_BACKOFF: Duration = Duration::from_secs(900);
const ARMED_MAX_BACKOFF: Duration = Duration::from_secs(120);
const RATE_LIMIT_HOLD: time::Duration = time::Duration::minutes(30);
/// Look back far enough to cover a slow Blink publication of the event.
const LOOKBACK: time::Duration = time::Duration::minutes(15);

/// Wait after a successful poll and the reported mode.
pub fn success_wait(armed: bool, conservative: bool, jitter_ms: i64) -> (Duration, &'static str) {
    if !armed {
        return (DISARMED, "idle");
    }
    if conservative {
        return (CONSERVATIVE_ARMED, "conservative");
    }
    let millis = FAST_ARMED_MS
        .saturating_add_signed(jitter_ms.clamp(-JITTER_MS, JITTER_MS))
        .max(MIN_ARMED_MS);
    (Duration::from_millis(millis), "fast")
}

/// Exponential failure backoff: 60 s, 120 s, ... up to 15 minutes, or up to
/// two minutes while armed unless Blink is rate limiting (`slow`).
pub fn next_backoff(previous: Duration, armed: bool, slow: bool) -> Duration {
    let cap = if armed && !slow {
        ARMED_MAX_BACKOFF
    } else {
        MAX_BACKOFF
    };
    (previous.max(CONSERVATIVE_ARMED) * 2).min(cap)
}

/// Run forever: poll while enrolled, faster while any network is armed.
pub async fn run(engine: EngineState) {
    let mut backoff = CONSERVATIVE_ARMED;
    let mut rate_limited_until: Option<OffsetDateTime> = None;
    loop {
        let armed = engine
            .client()
            .state()
            .await
            .networks
            .iter()
            .any(|network| network.armed == Some(true));
        let now = OffsetDateTime::now_utc();
        let limited = rate_limited_until.filter(|until| *until > now);
        let since = (now - LOOKBACK).format(&Rfc3339).unwrap_or_default();
        let (wait, state, mode, error) = match engine.client().motion_events(&since).await {
            Ok(events) => {
                backoff = CONSERVATIVE_ARMED;
                let fresh = engine.motion().absorb(events).await;
                crate::motion_recorder::handle(&engine, fresh).await;
                let jitter = rand::random_range(-JITTER_MS..=JITTER_MS);
                let (wait, mode) = success_wait(armed, limited.is_some(), jitter);
                (wait, if armed { "active" } else { "idle" }, mode, None)
            }
            Err(BlinkError::NotEnrolled) => (DISARMED, "disabled", "idle", None),
            Err(error) => {
                let rate_limited = error.is_rate_limited();
                if rate_limited {
                    rate_limited_until = Some(now + RATE_LIMIT_HOLD);
                }
                engine.motion().record_poll_failure();
                backoff = next_backoff(backoff, armed, rate_limited || limited.is_some());
                tracing::warn!(error = %error.redacted(), seconds = backoff.as_secs(),
                    "Blink motion poll failed");
                let state = if matches!(error, BlinkError::Authentication) {
                    "unauthorized"
                } else {
                    "backoff"
                };
                (backoff, state, "backoff", Some(error.redacted()))
            }
        };
        let rate_limited_until = rate_limited_until
            .filter(|until| *until > now)
            .and_then(|until| until.format(&Rfc3339).ok());
        engine
            .motion()
            .set_status(PollerStatus {
                state,
                interval_seconds: wait.as_secs(),
                mode,
                rate_limited_until,
                last_error: error,
                updated_at: None,
            })
            .await;
        tokio::time::sleep(wait).await;
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{ARMED_MAX_BACKOFF, JITTER_MS, MAX_BACKOFF, next_backoff, success_wait};

    #[test]
    fn armed_polling_is_fast_jittered_and_never_below_the_floor() {
        assert_eq!(
            success_wait(true, false, 0),
            (Duration::from_secs(15), "fast")
        );
        assert_eq!(
            success_wait(true, false, -JITTER_MS).0,
            Duration::from_secs(12)
        );
        assert_eq!(
            success_wait(true, false, JITTER_MS).0,
            Duration::from_secs(18)
        );
        // Out-of-range jitter is clamped; the floor holds even for huge values.
        assert_eq!(
            success_wait(true, false, -60_000).0,
            Duration::from_secs(12)
        );
        assert_eq!(success_wait(true, false, 60_000).0, Duration::from_secs(18));
    }

    #[test]
    fn rate_limits_restore_the_previous_interval_and_disarmed_stays_slow() {
        assert_eq!(
            success_wait(true, true, -JITTER_MS),
            (Duration::from_secs(30), "conservative")
        );
        assert_eq!(
            success_wait(false, false, 0),
            (Duration::from_secs(300), "idle")
        );
        assert_eq!(success_wait(false, true, 0).0, Duration::from_secs(300));
    }

    #[test]
    fn failures_back_off_exponentially_to_fifteen_minutes() {
        for (armed, slow) in [(false, false), (true, true), (false, true)] {
            let mut backoff = Duration::from_secs(30);
            let mut seen = Vec::new();
            for _ in 0..7 {
                backoff = next_backoff(backoff, armed, slow);
                seen.push(backoff.as_secs());
            }
            assert_eq!(seen, [60, 120, 240, 480, 900, 900, 900]);
        }
        assert_eq!(
            next_backoff(Duration::ZERO, false, false),
            Duration::from_secs(60)
        );
        assert_eq!(next_backoff(MAX_BACKOFF, false, false), MAX_BACKOFF);
    }

    #[test]
    fn armed_transient_failures_back_off_to_two_minutes_at_most() {
        let mut backoff = Duration::from_secs(30);
        let mut seen = Vec::new();
        for _ in 0..4 {
            backoff = next_backoff(backoff, true, false);
            seen.push(backoff.as_secs());
        }
        assert_eq!(seen, [60, 120, 120, 120]);
        // A long rate-limit backoff shrinks back once Blink stops limiting.
        assert_eq!(next_backoff(MAX_BACKOFF, true, false), ARMED_MAX_BACKOFF);
    }
}
