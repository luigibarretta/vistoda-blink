use std::time::Duration;

use axum::extract::ws::{Message as BrowserFrame, WebSocket};
use serde_json::{Value, json};
use tokio::time::{Instant, timeout};
use tokio_tungstenite::tungstenite::{Error as VendorError, Message};
use tracing::{info, warn};

use crate::{
    blink_webrtc::VendorSocket,
    blink_webrtc_frames::{self, Incoming},
    blink_webrtc_wire::{
        ServerEnvelope, answer_has_extra_e2ee, browser_event, ping_seconds, session_id,
        valid_event_session, valid_server,
    },
};

#[derive(Default)]
pub struct SessionState {
    pub session: Option<String>,
    pub activated: bool,
    pub mic_prepared: bool,
    pub mic_cooldown: Option<Instant>,
    pub candidates: usize,
    pub negotiated: bool,
}

#[derive(Default)]
pub struct ProviderUpdate {
    pub stop: bool,
    pub fallback: bool,
    pub pong: bool,
    pub ping_seconds: Option<u64>,
}

struct Translation {
    event: Option<Value>,
    update: ProviderUpdate,
}

pub async fn forward_provider(
    frame: Option<Result<Message, VendorError>>,
    provider: &mut VendorSocket,
    browser: &mut WebSocket,
    dialog: &str,
    doorbot: u64,
    state: &mut SessionState,
) -> ProviderUpdate {
    let envelope = match blink_webrtc_frames::provider(frame, provider).await {
        Incoming::Data(value) => value,
        Incoming::Heartbeat => return ProviderUpdate::default(),
        Incoming::Closed => {
            return ProviderUpdate {
                stop: true,
                ..Default::default()
            };
        }
    };
    let translation = match translate(&envelope, dialog, doorbot, state) {
        Ok(Some(value)) => value,
        Ok(None) => return ProviderUpdate::default(),
        Err(()) => {
            return ProviderUpdate {
                stop: true,
                ..Default::default()
            };
        }
    };
    let Some(event) = translation.event else {
        return translation.update;
    };
    if event.get("type").and_then(Value::as_str) == Some("fallback") {
        return ProviderUpdate {
            stop: true,
            fallback: true,
            ..translation.update
        };
    }
    if answer_has_extra_e2ee(&event) {
        let _ = ui(
            browser,
            json!({
                "type":"error", "message":"Modalità E2EE Blink non supportata"
            }),
        )
        .await;
        return ProviderUpdate {
            stop: true,
            ..translation.update
        };
    }
    let terminal = event.get("type").and_then(Value::as_str) == Some("closed");
    if terminal {
        info!(
            reason_code = event
                .get("code")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(-1),
            "Blink WebRTC provider closed the signaling session"
        );
    }
    let relay_failed = ui(browser, event).await.is_err();
    ProviderUpdate {
        stop: terminal || relay_failed,
        ..translation.update
    }
}

fn translate(
    envelope: &ServerEnvelope,
    dialog: &str,
    doorbot: u64,
    state: &mut SessionState,
) -> Result<Option<Translation>, ()> {
    if !correlate(envelope, dialog, doorbot, state)? {
        return Ok(None);
    }
    let update = ProviderUpdate {
        pong: envelope.method == "pong",
        ping_seconds: ping_seconds(&envelope.body),
        ..Default::default()
    };
    let Some(event) = browser_event(envelope) else {
        info!(method = %envelope.method,
            "Blink WebRTC received a provider event without a browser translation");
        return Ok(Some(Translation {
            event: None,
            update,
        }));
    };
    info!(method = %envelope.method, "Blink WebRTC accepted a provider signaling event");
    update_state(&event, state);
    Ok(Some(Translation {
        event: Some(event),
        update,
    }))
}

fn correlate(
    envelope: &ServerEnvelope,
    dialog: &str,
    doorbot: u64,
    state: &mut SessionState,
) -> Result<bool, ()> {
    if !valid_server(envelope, dialog, doorbot) {
        warn!(method = %envelope.method,
            has_doorbot = envelope.body.get("doorbot_id").is_some(),
            "Blink WebRTC ignored a provider event with invalid correlation");
        return Ok(false);
    }
    if let Some(value) = session_id(&envelope.body) {
        if state
            .session
            .as_deref()
            .is_some_and(|current| current != value)
        {
            return Err(());
        }
        state.session.get_or_insert_with(|| value.to_owned());
    }
    if !valid_event_session(envelope, state.session.as_deref()) {
        warn!(method = %envelope.method,
            has_session = session_id(&envelope.body).is_some(),
            "Blink WebRTC ignored a provider event with invalid session correlation");
        return Ok(false);
    }
    Ok(true)
}

fn update_state(event: &Value, state: &mut SessionState) {
    match event.get("type").and_then(Value::as_str) {
        Some("answer") => state.negotiated = true,
        Some("mic_overridden") => {
            let millis = event
                .get("cooldown_ms")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            state.mic_cooldown = Some(Instant::now() + Duration::from_millis(millis));
        }
        _ => {}
    }
}

pub async fn ui(socket: &mut WebSocket, value: Value) -> Result<(), ()> {
    timeout(
        Duration::from_secs(5),
        socket.send(BrowserFrame::Text(value.to_string().into())),
    )
    .await
    .map_err(|_| ())?
    .map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{SessionState, translate};
    use crate::blink_webrtc_wire::ServerEnvelope;

    #[test]
    fn preserves_pong_updates_without_a_browser_event() {
        let envelope = ServerEnvelope {
            dialog_id: "dialog".into(),
            method: "pong".into(),
            body: json!({"ping_interval": 12}),
        };
        let translation = translate(&envelope, "dialog", 42, &mut SessionState::default())
            .unwrap_or_else(|()| panic!("invalid pong"))
            .unwrap_or_else(|| panic!("missing pong update"));
        assert!(translation.event.is_none());
        assert!(translation.update.pong);
        assert_eq!(translation.update.ping_seconds, Some(12));
    }
}
