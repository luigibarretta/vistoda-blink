//! Native local Continue timer metadata; never a cloud extension command.
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Deserialize, Default)]
pub struct ProviderTiming {
    #[serde(default, deserialize_with = "optional_seconds")]
    pub continue_interval: Option<u64>,
    #[serde(default, deserialize_with = "optional_seconds")]
    pub continue_warning: Option<u64>,
    #[serde(default, deserialize_with = "optional_seconds")]
    pub duration: Option<u64>,
}
fn optional_seconds<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
    Ok(serde_json::Value::deserialize(d)?
        .as_u64()
        .filter(|value| *value <= 86_400))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionClock {
    started: Instant,
    continue_interval: u64,
    continue_warning: u64,
    duration: u64,
}
#[derive(Serialize)]
pub struct TimerMetadata {
    pub continue_interval: u64,
    pub continue_warning: u64,
    /// Effective absolute cap, not extended by frontend Continue.
    pub duration: u64,
    /// Accounts for viewers joining an already-running shared publisher.
    pub remaining_ms: u64,
}
impl ProviderTiming {
    pub fn start(&self, cap: Duration) -> SessionClock {
        // APK LiveVideoResponse defaults: 30 / 10 / 300 seconds.
        let duration = self
            .duration
            .filter(|value| *value > 0)
            .unwrap_or(300)
            .min(cap.as_secs());
        let interval = self
            .continue_interval
            .filter(|value| *value > 0)
            .unwrap_or(30)
            .min(duration);
        let warning = self
            .continue_warning
            .filter(|value| *value <= interval)
            .unwrap_or(10)
            .min(interval);
        SessionClock {
            started: Instant::now(),
            continue_interval: interval,
            continue_warning: warning,
            duration,
        }
    }
}
impl SessionClock {
    pub const fn duration(&self) -> Duration {
        Duration::from_secs(self.duration)
    }
    pub fn metadata(&self) -> TimerMetadata {
        self.at(Instant::now())
    }
    fn at(&self, now: Instant) -> TimerMetadata {
        let remaining = self
            .duration()
            .saturating_sub(now.saturating_duration_since(self.started));
        TimerMetadata {
            continue_interval: self.continue_interval,
            continue_warning: self.continue_warning,
            duration: self.duration,
            remaining_ms: u64::try_from(remaining.as_millis()).unwrap_or(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_and_provider_values_never_extend_engine_cap() -> Result<(), serde_json::Error> {
        let default = ProviderTiming::default().start(Duration::from_secs(600));
        assert_eq!(default.duration(), Duration::from_secs(300));
        let initial = default.at(default.started);
        assert_eq!(
            (
                initial.continue_interval,
                initial.continue_warning,
                initial.remaining_ms
            ),
            (30, 10, 300_000)
        );
        assert_eq!(
            default
                .at(default.started + Duration::from_secs(20))
                .remaining_ms,
            280_000
        );
        assert_eq!(
            default
                .at(default.started + Duration::from_secs(301))
                .remaining_ms,
            0
        );
        let provider: ProviderTiming = serde_json::from_str(
            r#"{"continue_interval":20,"continue_warning":5,"duration":400}"#,
        )?;
        let limited = provider.start(Duration::from_secs(75));
        assert_eq!(limited.duration(), Duration::from_secs(75));
        assert_eq!(limited.continue_interval, 20);
        assert_eq!(limited.continue_warning, 5);
        Ok(())
    }
    #[test]
    fn malformed_values_fall_back_and_warning_cannot_exceed_interval()
    -> Result<(), serde_json::Error> {
        for value in ["null", "-1", "1.5", "\"30\"", "{}", "999999"] {
            let provider: ProviderTiming = serde_json::from_str(&format!(
                r#"{{"continue_interval":{value},"continue_warning":{value},"duration":{value}}}"#
            ))?;
            let clock = provider.start(Duration::from_secs(75));
            assert_eq!(
                (
                    clock.continue_interval,
                    clock.continue_warning,
                    clock.duration
                ),
                (30, 10, 75)
            );
        }
        let provider: ProviderTiming =
            serde_json::from_str(r#"{"continue_interval":4,"continue_warning":50,"duration":0}"#)?;
        let clock = provider.start(Duration::from_secs(75));
        assert_eq!(
            (
                clock.continue_interval,
                clock.continue_warning,
                clock.duration
            ),
            (4, 4, 75)
        );
        Ok(())
    }
}
