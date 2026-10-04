//! Lightweight motion poller over the native v4 event list (ADR 0012).

use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::Duration,
};

use serde::Serialize;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::RwLock;

use crate::{blink_client::BlinkError, blink_media_v4::MotionEvent, hub::EngineState};

const ARMED_INTERVAL: Duration = Duration::from_secs(30);
const DISARMED_INTERVAL: Duration = Duration::from_secs(300);
const MAX_BACKOFF: Duration = Duration::from_secs(900);
/// Look back far enough to cover a slow Blink publication of the event.
const LOOKBACK: time::Duration = time::Duration::minutes(15);
/// A camera reports motion for this long after its latest event.
const MOTION_WINDOW: time::Duration = time::Duration::seconds(90);
const SEEN_LIMIT: usize = 1_024;
const THUMBNAIL_LIMIT: usize = 256;

#[derive(Clone, Debug, Default, Serialize)]
pub struct PollerStatus {
    pub state: &'static str,
    pub interval_seconds: u64,
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

    /// Thumbnail path of a recent event, when Blink published one.
    pub async fn thumbnail(&self, event_id: &str) -> Option<String> {
        self.inner.read().await.thumbnails.get(event_id).cloned()
    }

    /// Record events; returns only those first seen after the warm-up poll.
    async fn absorb(&self, events: Vec<MotionEvent>) -> Vec<MotionEvent> {
        let mut inner = self.inner.write().await;
        let warmed = inner.warmed;
        inner.warmed = true;
        let mut fresh = Vec::new();
        for event in events {
            if let Some(thumbnail) = &event.thumbnail
                && inner
                    .thumbnails
                    .insert(event.id.clone(), thumbnail.clone())
                    .is_none()
            {
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
        fresh
    }

    async fn set_status(&self, state: &'static str, interval: Duration, error: Option<String>) {
        self.inner.write().await.status = PollerStatus {
            state,
            interval_seconds: interval.as_secs(),
            last_error: error,
            updated_at: OffsetDateTime::now_utc().format(&Rfc3339).ok(),
        };
    }
}

/// Run forever: poll while enrolled, faster while any network is armed.
pub async fn run(engine: EngineState) {
    let mut backoff = ARMED_INTERVAL;
    loop {
        let armed = engine
            .client()
            .state()
            .await
            .networks
            .iter()
            .any(|network| network.armed == Some(true));
        let interval = if armed {
            ARMED_INTERVAL
        } else {
            DISARMED_INTERVAL
        };
        let since = (OffsetDateTime::now_utc() - LOOKBACK)
            .format(&Rfc3339)
            .unwrap_or_default();
        let wait = match engine.client().motion_events(&since).await {
            Ok(events) => {
                backoff = ARMED_INTERVAL;
                let fresh = engine.motion().absorb(events).await;
                let state = if armed { "active" } else { "idle" };
                engine.motion().set_status(state, interval, None).await;
                crate::motion_recorder::handle(&engine, fresh).await;
                interval
            }
            Err(BlinkError::NotEnrolled) => {
                engine
                    .motion()
                    .set_status("disabled", DISARMED_INTERVAL, None)
                    .await;
                DISARMED_INTERVAL
            }
            Err(error) => {
                let state = if matches!(error, BlinkError::Authentication) {
                    "unauthorized"
                } else {
                    "backoff"
                };
                backoff = (backoff * 2).min(MAX_BACKOFF);
                tracing::warn!(%error, seconds = backoff.as_secs(), "Blink motion poll failed");
                engine
                    .motion()
                    .set_status(state, backoff, Some(error.to_string()))
                    .await;
                backoff
            }
        };
        tokio::time::sleep(wait).await;
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
