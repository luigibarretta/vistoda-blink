//! Cached motion state fed by the v4 event poller (ADR 0012, ADR 0015).

use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use serde::Serialize;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::{Notify, RwLock};

use crate::{blink_media_v4::MotionEvent, hub::EngineState, motion_still::StillStore};

/// Upper bound of one `/v1/motion` long-poll.
pub const MAX_WAIT: Duration = Duration::from_secs(30);
/// A camera reports motion for this long after its latest event.
const MOTION_WINDOW: time::Duration = time::Duration::seconds(90);
const SEEN_LIMIT: usize = 1_024;
const THUMBNAIL_LIMIT: usize = 256;

#[derive(Clone, Debug, Default, Serialize)]
pub struct PollerStatus {
    pub state: &'static str,
    /// Effective wait before the next Blink poll, jitter included.
    pub interval_seconds: u64,
    /// `fast` (15 s while armed), `conservative` (30 s after a rate limit),
    /// `idle` (disarmed) or `backoff`.
    pub mode: &'static str,
    /// Conservative polling lasts until this time after Blink HTTP 429/403.
    pub rate_limited_until: Option<String>,
    pub last_error: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CameraMotion {
    pub event_id: String,
    pub last_motion_at: String,
    pub event_type: Option<String>,
    pub has_media: bool,
}

#[derive(Default)]
pub struct MotionTracker {
    inner: RwLock<Inner>,
    /// Bumped whenever a camera's latest event or still changes.
    sequence: AtomicU64,
    changed: Notify,
    pub(crate) stills: StillStore,
}

#[derive(Default)]
struct Inner {
    status: PollerStatus,
    seen: HashSet<String>,
    order: VecDeque<String>,
    warmed: bool,
    /// Latest event per Blink camera ID (or name when the ID is missing).
    cameras: HashMap<String, CameraMotion>,
    /// Thumbnail path per recent event; Blink may add it after the event.
    thumbnails: HashMap<String, String>,
    thumbnail_order: VecDeque<String>,
}

impl MotionTracker {
    pub async fn status(&self) -> PollerStatus {
        self.inner.read().await.status.clone()
    }

    /// Latest event for a camera and whether it is inside the motion window.
    pub async fn camera(&self, id: &str, name: &str) -> Option<(CameraMotion, bool)> {
        let inner = self.inner.read().await;
        let motion = inner
            .cameras
            .get(id)
            .or_else(|| inner.cameras.get(name))?
            .clone();
        let active = OffsetDateTime::parse(&motion.last_motion_at, &Rfc3339)
            .is_ok_and(|at| OffsetDateTime::now_utc() - at <= MOTION_WINDOW);
        Some((motion, active))
    }

    pub fn sequence(&self) -> u64 {
        self.sequence.load(Ordering::Acquire)
    }

    pub(crate) fn mark_changed(&self) {
        self.sequence.fetch_add(1, Ordering::AcqRel);
        self.changed.notify_waiters();
    }

    /// Long-poll: return once the sequence differs from `since` or `wait` ends.
    pub async fn wait_for_change(&self, since: u64, wait: Duration) {
        let notified = self.changed.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if self.sequence() != since {
            return;
        }
        let _ = tokio::time::timeout(wait.min(MAX_WAIT), notified).await;
    }

    /// Thumbnail path of a recent event, when Blink published one.
    pub async fn thumbnail(&self, event_id: &str) -> Option<String> {
        self.inner.read().await.thumbnails.get(event_id).cloned()
    }

    /// Record events; returns only those first seen after the warm-up poll.
    pub(crate) async fn absorb(&self, events: Vec<MotionEvent>) -> Vec<MotionEvent> {
        let mut inner = self.inner.write().await;
        let warmed = inner.warmed;
        inner.warmed = true;
        let mut fresh = Vec::new();
        let mut changed = false;
        for event in events {
            if let Some(thumbnail) = &event.thumbnail
                && inner
                    .thumbnails
                    .insert(event.id.clone(), thumbnail.clone())
                    .is_none()
            {
                changed = true;
                inner.thumbnail_order.push_back(event.id.clone());
                while inner.thumbnail_order.len() > THUMBNAIL_LIMIT {
                    if let Some(old) = inner.thumbnail_order.pop_front() {
                        inner.thumbnails.remove(&old);
                    }
                }
            }
            if !inner.seen.insert(event.id.clone()) {
                continue;
            }
            inner.order.push_back(event.id.clone());
            while inner.order.len() > SEEN_LIMIT {
                if let Some(old) = inner.order.pop_front() {
                    inner.seen.remove(&old);
                }
            }
            let key = event
                .device_id
                .clone()
                .unwrap_or_else(|| event.device_name.clone());
            let newer = inner
                .cameras
                .get(&key)
                .is_none_or(|current| current.last_motion_at < event.created_at);
            if newer {
                changed = true;
                inner.cameras.insert(
                    key,
                    CameraMotion {
                        event_id: event.id.clone(),
                        last_motion_at: event.created_at.clone(),
                        event_type: event.event_type.clone(),
                        has_media: event.has_media,
                    },
                );
            }
            if warmed {
                fresh.push(event);
            }
        }
        drop(inner);
        if changed {
            self.mark_changed();
        }
        fresh
    }

    pub(crate) async fn set_status(&self, status: PollerStatus) {
        self.inner.write().await.status = PollerStatus {
            updated_at: OffsetDateTime::now_utc().format(&Rfc3339).ok(),
            ..status
        };
    }
}

impl EngineState {
    pub fn motion(&self) -> &MotionTracker {
        &self.motion
    }
}

#[cfg(test)]
#[path = "motion_tests.rs"]
mod tests;
