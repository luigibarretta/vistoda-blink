//! Authenticated microphone-only relay. Existing HA video owns its own subscriber.
use crate::{
    api::{authorize, validate_alias},
    error::EngineError,
    hub::{EngineState, HubMessage, Subscriber},
    walnut_microphone::{self, Microphone},
};
use axum::{
    Router,
    extract::{
        Path, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::HeaderMap,
    response::Response,
    routing::get,
};
use futures_util::SinkExt;
use serde::Deserialize;
use serde_json::json;
use std::{io, sync::Arc, time::Duration};
use tokio::sync::Semaphore;

static SESSIONS: std::sync::LazyLock<Arc<Semaphore>> =
    std::sync::LazyLock::new(|| Arc::new(Semaphore::new(4)));

pub fn routes() -> Router<EngineState> {
    Router::new().route("/v1/cameras/{alias}/walnut", get(upgrade))
}

async fn upgrade(
    State(state): State<EngineState>,
    headers: HeaderMap,
    Path(alias): Path<String>,
    websocket: WebSocketUpgrade,
) -> Result<Response, EngineError> {
    authorize(&state, &headers)?;
    validate_alias(&alias)?;
    let permit = SESSIONS
        .clone()
        .try_acquire_owned()
        .map_err(|_| EngineError::PublisherBusy)?;
    let subscriber = state.subscribe(&alias).await?;
    Ok(websocket
        .max_message_size(2048)
        .max_frame_size(2048)
        .on_upgrade(move |mut browser| async move {
            let _permit = permit;
            let result =
                tokio::time::timeout(Duration::from_secs(600), session(&mut browser, subscriber))
                    .await;
            if !matches!(result, Ok(Ok(()))) {
                let _ = send(
                    &mut browser,
                    Message::Text(
                        json!({"type":"error",
                    "message":"Live audio Blink terminato; riaprire la sessione"})
                        .to_string()
                        .into(),
                    ),
                )
                .await;
            }
            let _ = tokio::time::timeout(Duration::from_secs(2), browser.close()).await;
        }))
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Control {
    Microphone { enabled: bool, request_id: u32 },
    Stop,
    Ping,
}

async fn send(browser: &mut WebSocket, message: Message) -> io::Result<()> {
    tokio::time::timeout(Duration::from_secs(2), browser.send(message))
        .await
        .map_err(io::Error::other)?
        .map_err(io::Error::other)
}

async fn session(browser: &mut WebSocket, mut subscriber: Subscriber) -> io::Result<()> {
    let runtime = subscriber.audio();
    let mut status = runtime.subscribe();
    status.mark_changed();
    let mut microphone = None;
    let mut microphone_request = None;
    let mut tick = tokio::time::interval(Duration::from_millis(250));
    let mut last_ping = tokio::time::Instant::now();
    let result = async {
        loop {
            tokio::select! {
                frame = browser.recv() => match frame {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<Control>(&text).map_err(io::Error::other)? {
                            Control::Stop => break,
                            Control::Ping => { last_ping = tokio::time::Instant::now(); }
                            Control::Microphone { enabled, request_id } => {
                                if let Some(old) = microphone.take() { old.stop().await; }
                                let mut reason = "";
                                if enabled {
                                    match Microphone::start(&runtime) {
                                        Ok(value) => microphone = Some(value),
                                        Err(error) => reason = walnut_microphone::errors::reason(&error),
                                    }
                                }
                                microphone_request = microphone.as_ref().map(|_| request_id);
                                send(browser, Message::Text(json!({"type":"microphone",
                                    "enabled":microphone.is_some(),
                                    "request_id":request_id,
                                    "reason":reason,
                                    "message":if enabled && microphone.is_none() {
                                        "Microfono occupato o non disponibile"
                                    } else { "" }}).to_string().into())).await?;
                            }
                        }
                    }
                    Some(Ok(Message::Binary(bytes))) => {
                        // An in-flight capture block may arrive after revocation.
                        // Never re-enable capture implicitly or interrupt video.
                        if bytes.len() != 1024 { return Err(io::Error::other("invalid PCM block")); }
                        if let Some(current) = microphone.as_mut()
                            && let Err(error) = current.pcm(&bytes).await {
                                if let Some(old) = microphone.take() { old.stop().await; }
                                send(browser, Message::Text(json!({"type":"microphone","enabled":false,
                                    "request_id":microphone_request.take(),
                                    "reason":walnut_microphone::errors::reason(&error),
                                    "message":"Microfono fermato: errore ingresso audio"})
                                    .to_string().into())).await?;
                        }
                    }
                    Some(Ok(Message::Ping(bytes))) => { send(browser, Message::Pong(bytes)).await?; }
                    Some(Ok(Message::Pong(_))) => {}
                    _ => break,
                },
                result = drain(&mut subscriber) => return result,
                result = walnut_microphone::forward(&mut microphone) => {
                    if let Err(error) = result {
                        if let Some(old) = microphone.take() { old.stop().await; }
                        send(browser, Message::Text(json!({"type":"microphone","enabled":false,
                            "request_id":microphone_request.take(),
                            "reason":walnut_microphone::errors::reason(&error),
                            "message":"Microfono fermato: errore codifica o disponibilità audio"})
                            .to_string().into())).await?;
                    }
                }
                changed = status.changed() => {
                    changed.map_err(io::Error::other)?;
                    let current = status.borrow_and_update().clone();
                    if microphone.is_some() && !current.microphone_enabled {
                        if let Some(old) = microphone.take() { old.stop().await; }
                        send(browser, Message::Text(json!({"type":"microphone","enabled":false,
                            "request_id":microphone_request.take(),"reason":"lease_revoked"})
                            .to_string().into())).await?;
                    }
                    send(browser, Message::Text(json!({"type":"audio_offer", "connected":current.connected,
                        "format":current.format,"supported":current.supported(),
                        "multi_client":current.multi_client,"audio_available":current.audio_available,
                        "sent_frames":current.sent_frames,
                        "session_timing":current.session_clock.as_ref().map(crate::blink_api::timing::SessionClock::metadata),
                        "stream_aec":current.format == Some(0xa000_0003)})
                        .to_string().into())).await?;
                },
                _ = tick.tick() => {
                    if last_ping.elapsed() > Duration::from_secs(25) { break; }
                    if microphone.as_ref().is_some_and(Microphone::idle) {
                        if let Some(old) = microphone.take() { old.stop().await; }
                        send(browser, Message::Text(json!({"type":"microphone","enabled":false,
                            "request_id":microphone_request.take(),"reason":"input_idle"})
                            .to_string().into())).await?;
                    }
                },
            }
        }
        Ok(())
    }.await;
    if let Some(old) = microphone {
        old.stop().await;
    }
    result
}

/// Hold the shared publisher lifetime without remuxing or sending video to HA.
/// A lag closes only this microphone session; an independent HLS subscriber lives on.
async fn drain(subscriber: &mut Subscriber) -> io::Result<()> {
    loop {
        match subscriber.recv().await.map_err(io::Error::other)? {
            HubMessage::Data(_) => {}
            HubMessage::End => return Ok(()),
        }
    }
}

#[cfg(test)]
#[path = "walnut_socket_tests.rs"]
mod tests;
