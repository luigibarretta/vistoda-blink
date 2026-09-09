use serde_json::json;

use crate::{
    blink_model::CameraState,
    blink_settings::{SettingKind, parse},
};

fn camera(kind: &str) -> CameraState {
    CameraState {
        id: "2".into(),
        network_id: "1".into(),
        alias: "balcone".into(),
        name: "Balcone".into(),
        serial: Some("ABC".into()),
        firmware: Some("10.73".into()),
        camera_type: kind.into(),
        product_type: kind.into(),
        enabled: Some(true),
        status: None,
        battery_state: Some("ok".into()),
        battery_voltage: None,
        battery_level: None,
        low_battery: Some(false),
        temperature_f: Some(86.0),
        wifi_dbm: Some(-51),
        motion_detected: false,
        thumbnail_url: None,
        powered: kind == "mini",
    }
}

#[test]
fn parses_only_allowlisted_settings_and_builds_a_stable_revision() {
    let response = json!({"camera": [{
        "enabled": true, "motion_sensitivity": 5, "alert_interval": 10,
        "video_length": 5, "video_quality": "standard",
        "video_quality_support": ["saver", "standard", "best"],
        "early_termination": true, "early_termination_supported": true,
        "early_notification": true, "early_notification_compatible": true,
        "record_audio_enable": true, "record_audio": true,
        "video_recording_enable": true, "video_recording_optional": true,
        "illuminator_enable": 2, "illuminator_intensity": 4,
        "account_secret": "must-never-leak"
    }]});
    let first = parse(&camera("default"), &response);
    let second = parse(&camera("default"), &response);
    assert_eq!(first.revision, second.revision);
    assert_eq!(first.revision.len(), 64);
    assert_eq!(first.settings.len(), 11);
    assert!(
        !serde_json::to_string(&first)
            .unwrap_or_default()
            .contains("account_secret")
    );
    let sensitivity = first
        .settings
        .iter()
        .find(|item| item.key == "motion_sensitivity");
    assert!(sensitivity.is_some_and(|item| item.kind == SettingKind::Integer && item.writable));
    let intensity = first
        .settings
        .iter()
        .find(|item| item.key == "ir_intensity");
    assert!(intensity.is_some_and(|item| !item.writable));
    let early = first
        .settings
        .iter()
        .find(|item| item.key == "early_notification");
    assert!(early.is_some_and(|item| item.value == json!(true)));
}

#[test]
fn normalizes_owl_night_vision_without_assuming_other_fields() {
    let settings = parse(&camera("mini"), &json!({"illuminator_enable": "auto"}));
    assert_eq!(settings.settings.len(), 1);
    assert_eq!(settings.settings[0].value, json!("auto"));
    assert!(settings.settings[0].writable);
}

#[test]
fn unsupported_camera_types_keep_safe_metadata_without_controls() {
    let settings = parse(&camera("doorbell"), &serde_json::Value::Null);
    assert!(settings.settings.is_empty());
    assert_eq!(settings.name, "Balcone");
    assert_eq!(settings.revision.len(), 64);
}
