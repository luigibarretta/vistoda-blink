use serde_json::json;

use crate::blink_webrtc_wire::{
    BrowserMessage, ServerEnvelope, browser_event, close, ice, live_view, valid_event_session,
};

#[test]
fn emits_native_lowercase_underscore_blink_fields() {
    let live = serde_json::to_value(live_view("dialog", 42, "v=0\r\n"))
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(live["method"], "live_view");
    assert_eq!(live["body"]["doorbot_id"], 42);
    assert_eq!(live["body"]["stream_options"]["audio_enabled"], false);
    let candidate = serde_json::to_value(ice("dialog", 42, None, "candidate:1", None, 0))
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(candidate["body"]["mlineindex"], 0);
    assert!(candidate["body"].get("session_id").is_none());
    assert!(
        serde_json::to_value(close("dialog", 42, None))
            .unwrap_or_else(|error| panic!("{error}"))["body"]["reason"]
            .is_object()
    );
}

#[test]
fn rejects_oversized_or_non_sdp_browser_start() {
    assert!(
        !BrowserMessage::Start {
            sdp: "invalid".into()
        }
        .validate()
    );
    assert!(
        BrowserMessage::Start {
            sdp: "v=0\r\n".into()
        }
        .validate()
    );
}

#[test]
fn bounds_provider_media_and_uses_safe_close_text() {
    let invalid = ServerEnvelope {
        dialog_id: "dialog".into(),
        method: "sdp".into(),
        body: json!({"type":"answer", "sdp":"invalid", "doorbot_id":42,
            "session_id":"session"}),
    };
    assert!(browser_event(&invalid).is_none());
    let closed = ServerEnvelope {
        dialog_id: "dialog".into(),
        method: "close".into(),
        body: json!({"reason":{"code":7,"text":"untrusted provider text"},
            "doorbot_id":42, "session_id":"session"}),
    };
    let event = browser_event(&closed).unwrap_or_else(|| panic!("missing close event"));
    assert_eq!(event["message"], "Sessione terminata dal provider Blink");
    assert!(!event.to_string().contains("untrusted"));
}

#[test]
fn only_the_official_legacy_close_code_requests_fallback() {
    let legacy = ServerEnvelope {
        dialog_id: "dialog".into(),
        method: "close".into(),
        body: json!({"reason":{"code":38,"text":"untrusted"},
            "doorbot_id":42, "session_id":"session"}),
    };
    let event = browser_event(&legacy).unwrap_or_else(|| panic!("missing legacy event"));
    assert_eq!(event["type"], "fallback");
    assert_eq!(event["reason"], "blink_legacy_device");
    assert!(!event.to_string().contains("untrusted"));

    let setup_failed = ServerEnvelope {
        dialog_id: "dialog".into(),
        method: "close".into(),
        body: json!({"reason":{"code":2}, "doorbot_id":42}),
    };
    let event = browser_event(&setup_failed).unwrap_or_else(|| panic!("missing close event"));
    assert_eq!(event["type"], "closed");
    assert_eq!(event["code"], 2);
}

#[test]
fn accepts_native_pre_session_close_and_microphone_override() {
    for method in ["close", "mic_overridden"] {
        let envelope = ServerEnvelope {
            dialog_id: "dialog".into(),
            method: method.into(),
            body: json!({"doorbot_id":42}),
        };
        assert!(valid_event_session(&envelope, None));
        assert!(valid_event_session(&envelope, Some("session")));
    }
    let mismatched = ServerEnvelope {
        dialog_id: "dialog".into(),
        method: "close".into(),
        body: json!({"doorbot_id":42, "session_id":"other"}),
    };
    assert!(!valid_event_session(&mismatched, Some("session")));
}
