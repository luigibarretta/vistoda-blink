use std::time::Duration;

use axum::{
    Router,
    extract::{
        Path, State, WebSocketUpgrade,
        ws::{Message as BrowserFrame, WebSocket},
    },
    http::HeaderMap,
    response::Response,
    routing::get,
};
use futures_util::StreamExt;
use serde_json::{Value, json};
use tokio::{
    net::TcpStream,
    time::{Instant, interval_at, timeout},
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async_with_config,
    tungstenite::protocol::WebSocketConfig,
};
use uuid::Uuid;

use crate::{
    api::{authorize, validate_alias},
    blink_webrtc_commands, blink_webrtc_frames,
    blink_webrtc_wire::{
        BrowserMessage, answer_has_extra_e2ee, browser_event, close, live_view, ping_seconds,
        session_command, session_id, valid_event_session, valid_server,
    },
    error::EngineError,
    hub::{EngineState, PublisherGuard},
};

const MAX_SESSION_SECONDS: u64 = 360;
const MAX_SIGNALING_BYTES: usize = 128 * 1024;
pub type VendorSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub fn routes() -> Router<EngineState> {
    Router::new().route("/v1/cameras/{alias}/webrtc", get(upgrade))
}

async fn upgrade(
    State(state): State<EngineState>,
    headers: HeaderMap,
    Path(alias): Path<String>,
    websocket: WebSocketUpgrade,
) -> Result<Response, EngineError> {
    authorize(&state, &headers)?;
    validate_alias(&alias)?;
    let lease = state.acquire_webrtc(&alias).await?;
    let (request, doorbot) = state.client().signaling_target(&alias).await?;
    Ok(websocket
        .max_message_size(128 * 1024)
        .on_upgrade(move |browser| session(browser, request, doorbot, lease)))
}

async fn session(
    mut browser: WebSocket,
    request: axum::http::Request<()>,
    doorbot: u64,
    _lease: PublisherGuard,
) {
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_SIGNALING_BYTES))
        .max_frame_size(Some(MAX_SIGNALING_BYTES))
        .max_write_buffer_size(256 * 1024);
    let provider = timeout(
        Duration::from_secs(12),
        connect_async_with_config(request, Some(config), false),
    )
    .await;
    let Ok(Ok((mut provider, _))) = provider else {
        let _ = ui(
            &mut browser,
            json!({"type":"error","message":"Segnalazione Blink non disponibile"}),
        )
        .await;
        return;
    };
    let Some(BrowserMessage::Start { sdp }) = initial(&mut browser).await else {
        return;
    };
    let dialog = Uuid::new_v4().to_string();
    if blink_webrtc_commands::send(&mut provider, &live_view(&dialog, doorbot, &sdp))
        .await
        .is_err()
    {
        return;
    }
    run(&mut browser, &mut provider, &dialog, doorbot).await;
}

async fn initial(browser: &mut WebSocket) -> Option<BrowserMessage> {
    let next = tokio::time::timeout(Duration::from_secs(15), browser.recv())
        .await
        .ok()??
        .ok()?;
    let BrowserFrame::Text(text) = next else {
        return None;
    };
    let message: BrowserMessage = serde_json::from_str(text.as_str()).ok()?;
    message.validate().then_some(message)
}

async fn run(browser: &mut WebSocket, provider: &mut VendorSocket, dialog: &str, doorbot: u64) {
    let mut session = None;
    let mut activated = false;
    let mut mic_prepared = false;
    let mut mic_cooldown = None;
    let mut candidates = 0_usize;
    let mut missed_pings = 0_u8;
    let mut negotiated = false;
    let mut ping = interval_at(
        Instant::now() + Duration::from_secs(10),
        Duration::from_secs(10),
    );
    let deadline = tokio::time::sleep(Duration::from_secs(MAX_SESSION_SECONDS));
    let negotiation = tokio::time::sleep(Duration::from_secs(30));
    tokio::pin!(deadline);
    tokio::pin!(negotiation);
    loop {
        tokio::select! {
            frame = browser.recv() => {
                let message = match blink_webrtc_frames::browser(frame, browser).await {
                    blink_webrtc_frames::Incoming::Data(value) => value,
                    blink_webrtc_frames::Incoming::Heartbeat => continue,
                    blink_webrtc_frames::Incoming::Closed => break,
                };
                if !message.validate() { break; }
                if blink_webrtc_commands::handle(provider, dialog, doorbot, session.as_deref(),
                    &mut activated, &mut mic_prepared, mic_cooldown, &mut candidates,
                    message).await { break; }
            }
            frame = provider.next() => {
                let envelope = match blink_webrtc_frames::provider(frame, provider).await {
                    blink_webrtc_frames::Incoming::Data(value) => value,
                    blink_webrtc_frames::Incoming::Heartbeat => continue,
                    blink_webrtc_frames::Incoming::Closed => break,
                };
                if !valid_server(&envelope, dialog, doorbot) { continue; }
                if envelope.method == "pong" { missed_pings = 0; }
                if let Some(seconds) = ping_seconds(&envelope.body) {
                    ping = interval_at(Instant::now() + Duration::from_secs(seconds),
                        Duration::from_secs(seconds));
                }
                if let Some(value) = session_id(&envelope.body) {
                    if session.as_deref().is_some_and(|current| current != value) { break; }
                    session.get_or_insert_with(|| value.to_owned());
                }
                if !valid_event_session(&envelope, session.as_deref()) { continue; }
                let Some(event) = browser_event(&envelope) else { continue };
                if event.get("type").and_then(Value::as_str) == Some("answer") {
                    negotiated = true;
                }
                if event.get("type").and_then(Value::as_str) == Some("mic_overridden") {
                    let millis = event.get("cooldown_ms").and_then(Value::as_u64).unwrap_or(0);
                    mic_cooldown = Some(Instant::now() + Duration::from_millis(millis));
                }
                if answer_has_extra_e2ee(&event) {
                    let _ = ui(browser, json!({"type":"error","message":"Modalità E2EE Blink non supportata"})).await;
                    break;
                }
                let closed = event.get("type").and_then(Value::as_str) == Some("closed");
                if ui(browser, event).await.is_err() || closed { break; }
            }
            _ = ping.tick(), if session.is_some() => {
                if missed_pings >= 2 {
                    let _ = ui(browser, json!({"type":"error","message":"Sessione Blink scaduta"})).await;
                    break;
                }
                let command = session_command(dialog, "ping", doorbot,
                    session.as_deref().unwrap_or_default(), &json!({}));
                if blink_webrtc_commands::send(provider, &command).await.is_err() { break; }
                missed_pings += 1;
            }
            () = &mut deadline => {
                let _ = ui(browser, json!({"type":"closed","code":0,"message":"Durata live conclusa"})).await;
                break;
            }
            () = &mut negotiation, if !negotiated => {
                let _ = ui(browser, json!({"type":"error","message":"Negoziazione Blink scaduta"})).await;
                break;
            }
        }
    }
    let _ =
        blink_webrtc_commands::send(provider, &close(dialog, doorbot, session.as_deref())).await;
    let _ = timeout(Duration::from_secs(5), provider.close(None)).await;
}

async fn ui(socket: &mut WebSocket, value: Value) -> Result<(), ()> {
    timeout(
        Duration::from_secs(5),
        socket.send(BrowserFrame::Text(value.to_string().into())),
    )
    .await
    .map_err(|_| ())?
    .map_err(|_| ())
}
