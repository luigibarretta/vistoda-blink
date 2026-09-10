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
use serde_json::json;
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
    blink_webrtc_commands,
    blink_webrtc_events::{SessionState, forward_provider, ui},
    blink_webrtc_frames,
    blink_webrtc_wire::{BrowserMessage, close, live_view, session_command},
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
    let mut state = SessionState::default();
    let mut missed_pings = 0_u8;
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
                if blink_webrtc_commands::handle(provider, dialog, doorbot, state.session.as_deref(),
                    &mut state.activated, &mut state.mic_prepared, state.mic_cooldown,
                    &mut state.candidates,
                    message).await { break; }
            }
            frame = provider.next() => {
                let update = forward_provider(frame, provider, browser, dialog, doorbot, &mut state).await;
                if update.stop { break; }
                if update.pong { missed_pings = 0; }
                if let Some(seconds) = update.ping_seconds {
                    ping = interval_at(Instant::now() + Duration::from_secs(seconds),
                        Duration::from_secs(seconds));
                }
            }
            _ = ping.tick(), if state.session.is_some() => {
                if missed_pings >= 2 {
                    let _ = ui(browser, json!({"type":"error","message":"Sessione Blink scaduta"})).await;
                    break;
                }
                let command = session_command(dialog, "ping", doorbot,
                    state.session.as_deref().unwrap_or_default(), &json!({}));
                if blink_webrtc_commands::send(provider, &command).await.is_err() { break; }
                missed_pings += 1;
            }
            () = &mut deadline => {
                let _ = ui(browser, json!({"type":"closed","code":0,"message":"Durata live conclusa"})).await;
                break;
            }
            () = &mut negotiation, if !state.negotiated => {
                let _ = ui(browser, json!({"type":"error","message":"Negoziazione Blink scaduta"})).await;
                break;
            }
        }
    }
    let _ =
        blink_webrtc_commands::send(provider, &close(dialog, doorbot, state.session.as_deref()))
            .await;
    let _ = timeout(Duration::from_secs(5), provider.close(None)).await;
}
