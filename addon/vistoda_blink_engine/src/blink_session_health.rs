//! Health of the Blink OAuth session and of its sealed credential file.
//!
//! Exposed through `/healthz` and `/metrics`; it never holds secrets.

use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use crate::{blink_error::BlinkError, oauth::OAuthError};

/// A transient refresh failure is not retried sooner than this, so callers
/// queued behind a failing refresh fail fast instead of each waiting for it.
const TRANSIENT_COOLDOWN: Duration = Duration::from_secs(10);
/// A refresh grant Blink rejected is not offered again sooner than this.
const REJECTED_COOLDOWN: Duration = Duration::from_secs(30 * 60);

struct Failure {
    generation: u64,
    at: Instant,
    rejected: bool,
}

#[derive(Default)]
pub struct SessionHealth {
    refresh_success: AtomicU64,
    refresh_failure: AtomicU64,
    refresh_rejected: AtomicU64,
    reauth_required: AtomicBool,
    persist_pending: AtomicBool,
    persist_failures: AtomicU64,
    persist_retrying: AtomicBool,
    failure: Mutex<Option<Failure>>,
}

/// Point-in-time copy for `/healthz` and `/metrics`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SessionHealthSnapshot {
    pub refresh_success: u64,
    pub refresh_failure: u64,
    pub refresh_rejected: u64,
    pub reauth_required: bool,
    pub persist_pending: bool,
    pub persist_failures: u64,
}

impl SessionHealth {
    pub fn snapshot(&self) -> SessionHealthSnapshot {
        SessionHealthSnapshot {
            refresh_success: self.refresh_success.load(Ordering::Relaxed),
            refresh_failure: self.refresh_failure.load(Ordering::Relaxed),
            refresh_rejected: self.refresh_rejected.load(Ordering::Relaxed),
            reauth_required: self.reauth_required.load(Ordering::Relaxed),
            persist_pending: self.persist_pending.load(Ordering::Relaxed),
            persist_failures: self.persist_failures.load(Ordering::Relaxed),
        }
    }

    pub fn refresh_succeeded(&self) {
        self.refresh_success.fetch_add(1, Ordering::Relaxed);
        self.reauth_required.store(false, Ordering::Relaxed);
        *self.lock_failure() = None;
    }

    pub fn refresh_failed(&self, generation: u64, error: &BlinkError) {
        let rejected = matches!(error, BlinkError::Authentication);
        if rejected {
            self.refresh_rejected.fetch_add(1, Ordering::Relaxed);
            self.reauth_required.store(true, Ordering::Relaxed);
        } else {
            self.refresh_failure.fetch_add(1, Ordering::Relaxed);
        }
        *self.lock_failure() = Some(Failure {
            generation,
            at: Instant::now(),
            rejected,
        });
    }

    /// A fresh enrollment or import replaced the credentials.
    pub fn credentials_replaced(&self) {
        self.reauth_required.store(false, Ordering::Relaxed);
        *self.lock_failure() = None;
    }

    /// The error of a refresh that failed moments ago for this generation.
    pub fn recent_failure(&self, generation: u64) -> Option<BlinkError> {
        let failure = self.lock_failure();
        let failure = failure
            .as_ref()
            .filter(|item| item.generation == generation)?;
        if failure.rejected {
            (failure.at.elapsed() < REJECTED_COOLDOWN).then_some(BlinkError::Authentication)
        } else {
            (failure.at.elapsed() < TRANSIENT_COOLDOWN)
                .then_some(BlinkError::OAuth(OAuthError::Unexpected))
        }
    }

    /// Returns true when the file was out of sync before this write.
    pub fn persisted(&self) -> bool {
        self.persist_pending.swap(false, Ordering::Relaxed)
    }

    pub fn persist_failed(&self) {
        self.persist_failures.fetch_add(1, Ordering::Relaxed);
        self.persist_pending.store(true, Ordering::Relaxed);
    }

    pub fn persist_pending(&self) -> bool {
        self.persist_pending.load(Ordering::Relaxed)
    }

    /// Claims the single retry task; false when one already runs.
    pub fn start_retry(&self) -> bool {
        !self.persist_retrying.swap(true, Ordering::Relaxed)
    }

    pub fn stop_retry(&self) {
        self.persist_retrying.store(false, Ordering::Relaxed);
    }

    fn lock_failure(&self) -> std::sync::MutexGuard<'_, Option<Failure>> {
        self.failure
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::SessionHealth;
    use crate::blink_error::BlinkError;

    #[test]
    fn rejection_latches_reauth_until_success_or_new_credentials() {
        let health = SessionHealth::default();
        health.refresh_failed(3, &BlinkError::Authentication);
        assert!(health.snapshot().reauth_required);
        assert!(matches!(
            health.recent_failure(3),
            Some(BlinkError::Authentication)
        ));
        assert!(health.recent_failure(4).is_none());
        health.credentials_replaced();
        assert!(!health.snapshot().reauth_required);
        health.refresh_failed(4, &BlinkError::InvalidResponse);
        assert_eq!(health.snapshot().refresh_failure, 1);
        assert!(matches!(
            health.recent_failure(4),
            Some(BlinkError::OAuth(_))
        ));
        health.refresh_succeeded();
        assert!(health.recent_failure(4).is_none());
    }

    #[test]
    fn persist_flag_tracks_disk_and_memory_disagreement() {
        let health = SessionHealth::default();
        assert!(!health.persisted());
        health.persist_failed();
        assert!(health.snapshot().persist_pending);
        assert!(health.start_retry() && !health.start_retry());
        assert!(health.persisted());
        assert!(!health.snapshot().persist_pending);
        assert_eq!(health.snapshot().persist_failures, 1);
    }
}
