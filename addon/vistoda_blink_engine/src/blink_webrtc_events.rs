use std::time::Duration;

use axum::extract::ws::{Message as BrowserFrame, WebSocket};
use serde_json::{Value, json};
use tokio::time::{Instant, timeout};
use tokio_tungstenite::tungstenite::{Error as VendorError, Message};

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
    pub pong: bool,
    pub ping_seconds: Option<u64>,
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
    let (event, update) = match translate(&envelope, dialog, doorbot, state) {
        Ok(Some(value)) => value,
        Ok(None) => return ProviderUpdate::default(),
        Err(()) => {
            return ProviderUpdate {
                stop: true,
                ..Default::default()
            };
        }
    };
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
            ..update
        };
    }
    let closed = event.get("type").and_then(Value::as_str) == Some("closed");
    ProviderUpdate {
        stop: closed || ui(browser, event).await.is_err(),
        ..update
    }
}

fn translate(
    envelope: &ServerEnvelope,
    dialog: &str,
    doorbot: u64,
    state: &mut SessionState,
) -> Result<Option<(Value, ProviderUpdate)>, ()> {
    if !valid_server(envelope, dialog, doorbot) {
        return Ok(None);
    }
    let update = ProviderUpdate {
        pong: envelope.method == "pong",
        ping_seconds: ping_seconds(&envelope.body),
        ..Default::default()
    };
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
        return Ok(None);
    }
    let Some(event) = browser_event(envelope) else {
        return Ok(None);
    };
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
    Ok(Some((event, update)))
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
