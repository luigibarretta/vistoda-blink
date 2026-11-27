use std::{
    collections::HashMap,
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

use crate::{blink_client::BlinkClient, error::EngineError, hub::PublisherGuard, live};

/// Cameras for which Blink refused a live view (HTTP 409: busy, or another live is open).
///
/// Home Assistant retries a failed live every few seconds; asking Blink again at once only
/// repeats the refusal. For a short while the engine answers "try again in N seconds"
/// without contacting Blink.
#[derive(Default)]
pub struct LiveCooldowns {
    until: Mutex<HashMap<String, Instant>>,
}

impl LiveCooldowns {
    /// Refuses a live view for a camera that Blink refused a moment ago (HTTP 429 + Retry-After).
    pub fn ensure_open(&self, alias: &str) -> Result<(), EngineError> {
        match self.remaining(alias, Instant::now()) {
            Some(wait) => Err(EngineError::LiveCoolingDown(wait)),
            None => Ok(()),
        }
    }

    /// Runs one live session in the background and remembers a refusal by Blink.
    pub fn run(self: &Arc<Self>, client: BlinkClient, alias: String, guard: PublisherGuard) {
        let cooldowns = Arc::clone(self);
        tokio::spawn(async move {
            if let Some(wait) = live::produce(client, alias.clone(), guard).await {
                cooldowns.start(&alias, wait);
            }
        });
    }

    pub fn start(&self, alias: &str, wait: Duration) {
        self.start_at(alias, Instant::now(), wait);
    }

    fn start_at(&self, alias: &str, now: Instant, wait: Duration) {
        self.until
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(alias.to_owned(), now + wait);
    }

    /// Whole seconds (at least 1) left for `alias`, or `None` when it may be asked again.
    pub fn remaining(&self, alias: &str, now: Instant) -> Option<u64> {
        let mut until = self.until.lock().unwrap_or_else(PoisonError::into_inner);
        let end = *until.get(alias)?;
        let left = end
            .checked_duration_since(now)
            .filter(|left| !left.is_zero());
        if left.is_none() {
            until.remove(alias);
        }
        left.map(|left| left.as_secs() + u64::from(left.subsec_nanos() > 0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refused_camera_waits_and_then_may_be_asked_again() {
        let cooldowns = LiveCooldowns::default();
        let now = Instant::now();
        assert_eq!(cooldowns.remaining("front", now), None);
        cooldowns.start_at("front", now, Duration::from_secs(30));
        assert_eq!(cooldowns.remaining("front", now), Some(30));
        assert_eq!(
            cooldowns.remaining("front", now + Duration::from_millis(29_500)),
            Some(1)
        );
        assert_eq!(
            cooldowns.remaining("front", now + Duration::from_secs(30)),
            None
        );
        assert_eq!(
            cooldowns.remaining("front", now),
            None,
            "an expired wait is forgotten"
        );
    }

    #[test]
    fn the_wait_belongs_to_one_camera() {
        let cooldowns = LiveCooldowns::default();
        let now = Instant::now();
        cooldowns.start_at("front", now, Duration::from_secs(10));
        assert_eq!(cooldowns.remaining("back", now), None);
    }
}
