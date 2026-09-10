use futures_util::SinkExt;
use serde::Serialize;
use serde_json::{Value, json};
use tokio::time::{Duration, Instant, timeout};
use tokio_tungstenite::tungstenite::Message;

use crate::{
    blink_webrtc::VendorSocket,
    blink_webrtc_wire::{BrowserMessage, ice, session_command},
};

const MAX_CANDIDATES: usize = 256;

#[allow(clippy::too_many_arguments)]
pub async fn handle(
    provider: &mut VendorSocket,
    dialog: &str,
    doorbot: u64,
    session: Option<&str>,
    activated: &mut bool,
    mic_prepared: &mut bool,
    mic_cooldown: Option<Instant>,
    candidates: &mut usize,
    message: BrowserMessage,
) -> bool {
    let command = match message {
        BrowserMessage::Ice {
            candidate,
            sdp_mid,
            sdp_mline_index,
        } => {
            *candidates += 1;
            if *candidates > MAX_CANDIDATES {
                return true;
            }
            ice(
                dialog,
                doorbot,
                session,
                &candidate,
                sdp_mid.as_deref(),
                sdp_mline_index,
            )
        }
        BrowserMessage::Sdp { sdp, reason } => {
            let Some(id) = session else { return false };
            let mut values = json!({"sdp": sdp, "type": "offer"});
            if let (Some(target), Some(reason)) = (values.as_object_mut(), reason) {
                target.insert("reason".into(), Value::String(reason));
            }
            session_command(dialog, "sdp", doorbot, id, &values)
        }
        BrowserMessage::Activate if !*activated => {
            let Some(id) = session else { return false };
            *activated = true;
            session_command(dialog, "activate_session", doorbot, id, &json!({}))
        }
        BrowserMessage::Microphone { enabled } => {
            let Some(id) = session else { return false };
            if enabled && !*activated {
                return false;
            }
            if enabled && mic_cooldown.is_some_and(|until| Instant::now() < until) {
                return false;
            }
            if enabled && !*mic_prepared {
                let options = session_command(
                    dialog,
                    "camera_options",
                    doorbot,
                    id,
                    &json!({"stealth_mode": false}),
                );
                if send(provider, &options).await.is_err() {
                    return true;
                }
                *mic_prepared = true;
            }
            session_command(
                dialog,
                "mic_enable",
                doorbot,
                id,
                &json!({"enabled": enabled}),
            )
        }
        BrowserMessage::Speaker { enabled } => {
            let Some(id) = session else { return false };
            session_command(
                dialog,
                "stream_options",
                doorbot,
                id,
                &json!({"audio_enabled": enabled}),
            )
        }
        BrowserMessage::Stop => return true,
        BrowserMessage::Start { .. } | BrowserMessage::Activate => return false,
    };
    send(provider, &command).await.is_err()
}

pub async fn send(socket: &mut VendorSocket, value: &(impl Serialize + Sync)) -> Result<(), ()> {
    let text = serde_json::to_string(value).map_err(|_| ())?;
    timeout(
        Duration::from_secs(5),
        socket.send(Message::Text(text.into())),
    )
    .await
    .map_err(|_| ())?
    .map_err(|_| ())
}
