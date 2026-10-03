//! Motion-triggered recordings: a rolling buffer that never touches manual ones.

use std::sync::Arc;

use super::{RecordingManager, RecordingManifest};
use crate::{error::EngineError, hub::EngineState};

pub const MOTION_TRIGGER: &str = "motion";

impl RecordingManager {
    /// Start one motion recording keyed by the provider event (idempotent).
    /// When the spool lacks headroom, the oldest completed motion recordings
    /// are removed first; manual recordings are never removed automatically.
    pub async fn start_motion(
        self: &Arc<Self>,
        engine: EngineState,
        camera: &str,
        duration_seconds: u64,
        event_id: &str,
    ) -> Result<RecordingManifest, EngineError> {
        let request_id = format!("motion-{event_id}");
        loop {
            match self
                .start(
                    engine.clone(),
                    camera,
                    duration_seconds,
                    &request_id,
                    Some(MOTION_TRIGGER),
                )
                .await
            {
                Err(EngineError::RecordingCapacity) => {
                    if !self.evict_oldest_motion().await? {
                        return Err(EngineError::RecordingCapacity);
                    }
                }
                result => return result,
            }
        }
    }

    /// Remove the oldest completed motion recording; `false` when none is left.
    async fn evict_oldest_motion(&self) -> Result<bool, EngineError> {
        let oldest = {
            let state = self.state.lock().await;
            state
                .manifests
                .values()
                .filter(|item| {
                    item.trigger.as_deref() == Some(MOTION_TRIGGER)
                        && matches!(item.status.as_str(), "ready" | "failed")
                })
                .min_by(|left, right| left.requested_at.cmp(&right.requested_at))
                .map(|item| item.recording_id.clone())
        };
        match oldest {
            Some(id) => {
                tracing::info!("removing the oldest motion recording to free spool space");
                self.acknowledge(&id).await
            }
            None => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MOTION_TRIGGER, RecordingManager, RecordingManifest};

    fn manifest(id: &str, requested_at: &str, trigger: Option<&str>) -> RecordingManifest {
        let mut manifest = RecordingManifest::pending("cucina", 15, trigger);
        manifest.recording_id = id.into();
        manifest.requested_at = requested_at.into();
        manifest.status = "ready".into();
        manifest
    }

    #[tokio::test]
    async fn evicts_only_the_oldest_motion_recording() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let manager = RecordingManager::load(directory.path().join("recordings"), 60, 1, 10)?;
        {
            let mut state = manager.state.lock().await;
            for item in [
                manifest("manual-old", "2026-10-01T00:00:00Z", None),
                manifest("motion-new", "2026-10-03T00:00:00Z", Some(MOTION_TRIGGER)),
                manifest("motion-old", "2026-10-02T00:00:00Z", Some(MOTION_TRIGGER)),
            ] {
                state.manifests.insert(item.recording_id.clone(), item);
            }
        }
        assert!(manager.evict_oldest_motion().await?);
        assert!(manager.evict_oldest_motion().await?);
        assert!(
            !manager.evict_oldest_motion().await?,
            "manual recordings are never evicted"
        );
        let remaining = manager.list().await;
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].recording_id, "manual-old");
        Ok(())
    }
}
