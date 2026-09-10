use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const MAX_SDP_BYTES: usize = 96 * 1024;
const MAX_CANDIDATE_BYTES: usize = 4096;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BrowserMessage {
    Start {
        sdp: String,
    },
    Ice {
        candidate: String,
        sdp_mid: Option<String>,
        sdp_mline_index: u16,
    },
    Sdp {
        sdp: String,
        reason: Option<String>,
    },
    Microphone {
        enabled: bool,
    },
    Speaker {
        enabled: bool,
    },
    Activate,
    Stop,
}

#[derive(Debug, Deserialize)]
pub struct ServerEnvelope {
    pub dialog_id: String,
    pub method: String,
    #[serde(default)]
    pub body: Value,
}

#[derive(Serialize)]
pub struct ClientEnvelope {
    pub dialog_id: String,
    pub method: String,
    pub body: Value,
}

impl BrowserMessage {
    pub fn validate(&self) -> bool {
        match self {
            Self::Start { sdp } => valid_sdp(sdp),
            Self::Ice {
                candidate, sdp_mid, ..
            } => {
                !candidate.is_empty()
                    && candidate.len() <= MAX_CANDIDATE_BYTES
                    && sdp_mid.as_ref().is_none_or(|value| value.len() <= 64)
            }
            Self::Sdp { sdp, reason } => {
                valid_sdp(sdp) && reason.as_ref().is_none_or(|value| value.len() <= 64)
            }
            Self::Microphone { .. } | Self::Speaker { .. } | Self::Activate | Self::Stop => true,
        }
    }
}

pub fn live_view(dialog: &str, doorbot: u64, sdp: &str) -> ClientEnvelope {
    envelope(
        dialog,
        "live_view",
        json!({
            "doorbot_id": doorbot,
            "sdp": sdp,
            "stream_options": {"video_enabled": true, "audio_enabled": false}
        }),
    )
}

pub fn ice(
    dialog: &str,
    doorbot: u64,
    session: Option<&str>,
    candidate: &str,
    mid: Option<&str>,
    line: u16,
) -> ClientEnvelope {
    let mut body = json!({
        "doorbot_id": doorbot, "ice": candidate,
        "mid": mid.unwrap_or("0"), "mlineindex": line
    });
    insert_session(&mut body, session);
    envelope(dialog, "ice", body)
}

pub fn session_command(
    dialog: &str,
    method: &str,
    doorbot: u64,
    session: &str,
    values: &Value,
) -> ClientEnvelope {
    let mut body = json!({"doorbot_id": doorbot, "session_id": session});
    if let (Some(target), Some(source)) = (body.as_object_mut(), values.as_object()) {
        target.extend(source.clone());
    }
    envelope(dialog, method, body)
}

pub fn close(dialog: &str, doorbot: u64, session: Option<&str>) -> ClientEnvelope {
    let mut body = json!({
        "doorbot_id": doorbot, "reason": {"code": 0, "text": "client_closed"}
    });
    insert_session(&mut body, session);
    envelope(dialog, "close", body)
}

pub fn valid_server(envelope: &ServerEnvelope, dialog: &str, doorbot: u64) -> bool {
    if envelope.dialog_id != dialog {
        return false;
    }
    match envelope.method.as_str() {
        "pong" => body_u64(&envelope.body, "doorbot_id").is_none_or(|value| value == doorbot),
        _ => body_u64(&envelope.body, "doorbot_id") == Some(doorbot),
    }
}

pub fn valid_event_session(envelope: &ServerEnvelope, expected: Option<&str>) -> bool {
    let actual = session_id(&envelope.body);
    if actual.is_some() && expected.is_some() && actual != expected {
        return false;
    }
    match envelope.method.as_str() {
        "sdp" | "ice" | "session_started" | "stream_duration_warning" | "ice_restart" => {
            actual.is_some()
        }
        _ => true,
    }
}

pub fn session_id(body: &Value) -> Option<&str> {
    body.get("session_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 128)
}

pub fn ping_seconds(body: &Value) -> Option<u64> {
    body.pointer("/session_info/ping_interval")
        .or_else(|| body.get("ping_interval"))
        .and_then(Value::as_u64)
        .filter(|value| (3..=60).contains(value))
}

pub fn browser_event(envelope: &ServerEnvelope) -> Option<Value> {
    let body = &envelope.body;
    match envelope.method.as_str() {
        "sdp" if body.get("type")?.as_str()? == "answer" => {
            let sdp = body.get("sdp")?.as_str()?;
            valid_sdp(sdp).then(|| json!({"type": "answer", "sdp": sdp}))
        }
        "ice" => {
            let candidate = body.get("ice")?.as_str()?;
            let mid = body.get("mid").and_then(Value::as_str);
            let line = body.get("mlineindex").and_then(Value::as_u64).unwrap_or(0);
            (candidate.len() <= MAX_CANDIDATE_BYTES
                && !candidate.is_empty()
                && mid.is_none_or(|value| value.len() <= 64)
                && line <= 32)
                .then(|| {
                    json!({
                        "type": "ice", "candidate": candidate,
                        "sdp_mid": mid, "sdp_mline_index": line
                    })
                })
        }
        "session_created" | "session_started" => Some(json!({"type": envelope.method})),
        "mic_overridden" => Some(json!({
            "type": "mic_overridden",
            "cooldown_ms": body.get("cooldown_ms").and_then(Value::as_u64).unwrap_or(0)
        })),
        "close" => Some(json!({
            "type": "closed",
            "code": body.pointer("/reason/code").and_then(Value::as_i64).unwrap_or(0),
            "message": "Sessione terminata dal provider Blink"
        })),
        "stream_duration_warning" => Some(json!({
            "type": "duration_warning",
            "warning_timeout": body.get("warning_timeout").and_then(Value::as_u64)
        })),
        "ice_restart" => Some(json!({"type": "ice_restart"})),
        _ => None,
    }
}

pub fn answer_has_extra_e2ee(event: &Value) -> bool {
    event.get("type").and_then(Value::as_str) == Some("answer")
        && event
            .get("sdp")
            .and_then(Value::as_str)
            .is_some_and(|sdp| sdp.contains("a=e2ee-content-encryption-mode:"))
}

fn envelope(dialog_id: &str, method: &str, body: Value) -> ClientEnvelope {
    ClientEnvelope {
        dialog_id: dialog_id.to_owned(),
        method: method.to_owned(),
        body,
    }
}

fn valid_sdp(sdp: &str) -> bool {
    sdp.len() <= MAX_SDP_BYTES && sdp.starts_with("v=0") && !sdp.contains('\0')
}

fn body_u64(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(|item| {
        item.as_u64()
            .or_else(|| item.as_str().and_then(|text| text.parse::<u64>().ok()))
    })
}

fn insert_session(body: &mut Value, session: Option<&str>) {
    if let (Some(target), Some(value)) = (body.as_object_mut(), session) {
        target.insert("session_id".into(), Value::String(value.to_owned()));
    }
}
