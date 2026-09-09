use super::{child_command_id, has_readable_media, parse_manifest, storage_status};
use serde_json::json;

#[test]
fn parses_only_read_only_status_and_bounded_clip_metadata() {
    let status = storage_status(&json!({"enabled": true, "usb_state": "mounted",
        "usb_storage_used": 31, "usb_storage_full": false, "can_format_usb": true}));
    assert!(status.enabled);
    assert_eq!(status.usb_state, "mounted");
    assert_eq!(status.usb_storage_used, Some(31));
    let (manifest, clips) = parse_manifest(&json!({"manifest_id": 8, "media": [{"id": 9,
        "device_name": "Balcone", "created_at": "2026-09-09T10:00:00Z",
        "clip_length_ms": 5000, "media": "/request/9"}, {"id": 10,
        "clip_start_millis": 1_788_948_000_000_u64}]}));
    assert_eq!(manifest, Some(8));
    assert_eq!(clips[0].id, 9);
    assert!(clips[0].media_available);
    assert_eq!(clips[1].created_at, "1788948000000");
}

#[test]
fn extracts_manifest_child_command() {
    assert_eq!(
        child_command_id(&json!({"commands": [{"id": 73}]})),
        Some(73)
    );
}

#[test]
fn requests_manifests_only_for_official_readable_usb_states() {
    assert!(has_readable_media("active"));
    assert!(has_readable_media("memory_full"));
    assert!(!has_readable_media("unmounted"));
    assert!(!has_readable_media("format_required"));
    assert!(!has_readable_media("unavailable"));
    assert!(!has_readable_media("incompatible"));
}
