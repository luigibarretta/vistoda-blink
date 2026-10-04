//! Start HA-local recordings for new motion events on armed networks.

use crate::{blink_media_v4::MotionEvent, error::EngineError, hub::EngineState};

pub async fn handle(engine: &EngineState, events: Vec<MotionEvent>) {
    if events.is_empty() {
        return;
    }
    let settings = engine.motion_settings().load();
    if !settings.enabled {
        return;
    }
    let state = engine.client().state().await;
    for event in events {
        let Some(camera) = state.cameras.iter().find(|camera| {
            event.device_id.as_deref() == Some(camera.id.as_str())
                || (event.device_id.is_none() && camera.name == event.device_name)
        }) else {
            continue;
        };
        let armed = state
            .networks
            .iter()
            .any(|network| network.id == camera.network_id && network.armed == Some(true));
        if !armed || !settings.selects(&camera.alias) {
            continue;
        }
        let result = engine
            .recordings()
            .start_motion(
                engine.clone(),
                &camera.alias,
                settings.duration_seconds,
                &event.id,
            )
            .await;
        match result {
            Ok(_) => tracing::info!(camera = %camera.alias, "motion recording started"),
            // One recorder per camera: a burst of events extends nothing, but
            // each event still gets a frame of the recording already running.
            Err(EngineError::RecordingActive) => {}
            Err(error) => {
                tracing::warn!(camera = %camera.alias, %error, "motion recording not started");
                continue;
            }
        }
        // Fallback image for events without a Blink thumbnail (ADR 0015).
        crate::motion_still::spawn(engine.clone(), camera.alias.clone(), event.id.clone());
    }
}
