use axum::extract::ws::{Message as BrowserFrame, WebSocket};
use futures_util::SinkExt;
use tokio_tungstenite::tungstenite::Message;
use tracing::warn;

use crate::blink_webrtc::VendorSocket;
use crate::blink_webrtc_wire::{BrowserMessage, ServerEnvelope};

pub enum Incoming<T> {
    Data(T),
    Heartbeat,
    Closed,
}

pub async fn browser(
    frame: Option<Result<BrowserFrame, axum::Error>>,
    socket: &mut WebSocket,
) -> Incoming<BrowserMessage> {
    match frame {
        Some(Ok(BrowserFrame::Text(text))) => {
            serde_json::from_str(text.as_str()).map_or(Incoming::Closed, Incoming::Data)
        }
        Some(Ok(BrowserFrame::Ping(value))) => {
            if socket.send(BrowserFrame::Pong(value)).await.is_err() {
                Incoming::Closed
            } else {
                Incoming::Heartbeat
            }
        }
        Some(Ok(BrowserFrame::Pong(_))) => Incoming::Heartbeat,
        Some(Ok(BrowserFrame::Close(_) | BrowserFrame::Binary(_)) | Err(_)) | None => {
            Incoming::Closed
        }
    }
}

pub async fn provider(
    frame: Option<Result<Message, tokio_tungstenite::tungstenite::Error>>,
    socket: &mut VendorSocket,
) -> Incoming<ServerEnvelope> {
    match frame {
        Some(Ok(Message::Text(text))) => match serde_json::from_str(text.as_str()) {
            Ok(value) => Incoming::Data(value),
            Err(error) => {
                let keys = serde_json::from_str::<serde_json::Value>(text.as_str())
                    .ok()
                    .and_then(|value| {
                        value.as_object().map(|map| {
                            let mut keys = map.keys().cloned().collect::<Vec<_>>();
                            keys.sort();
                            keys.join(",")
                        })
                    })
                    .unwrap_or_else(|| "non_object".to_owned());
                warn!(%error, top_level_keys = %keys,
                        "Blink WebRTC ignored an unrecognized provider envelope");
                Incoming::Heartbeat
            }
        },
        Some(Ok(Message::Ping(value))) => {
            if socket.send(Message::Pong(value)).await.is_err() {
                Incoming::Closed
            } else {
                Incoming::Heartbeat
            }
        }
        Some(Ok(Message::Pong(_))) => Incoming::Heartbeat,
        Some(Ok(Message::Binary(value))) => {
            warn!(
                frame_bytes = value.len(),
                "Blink WebRTC received unsupported binary signaling"
            );
            Incoming::Heartbeat
        }
        Some(Ok(Message::Close(_) | Message::Frame(_)) | Err(_)) | None => Incoming::Closed,
    }
}
