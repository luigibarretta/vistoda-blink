use super::{
    CameraAction, camera_action, camera_legacy_zones, camera_zones, live_command, temperature_alert,
};

#[test]
fn uses_the_current_homescreen_contract() {
    assert_eq!(super::homescreen("9"), "/api/v4/accounts/9/homescreen");
}
use crate::blink_model::CameraState;

fn camera(kind: &str) -> CameraState {
    CameraState {
        id: "2".into(),
        network_id: "1".into(),
        alias: "x".into(),
        name: "X".into(),
        serial: None,
        firmware: None,
        camera_type: kind.into(),
        product_type: kind.into(),
        enabled: None,
        status: None,
        battery_state: None,
        battery_voltage: None,
        battery_level: None,
        low_battery: None,
        temperature_f: None,
        wifi_dbm: None,
        motion_detected: false,
        thumbnail_url: None,
        powered: false,
        preferred_live_transport: crate::blink_model::LiveTransport::default(),
        ring_device_id: None,
        two_way_audio: None,
        audio_aec: None,
        audio_privacy_enabled: None,
    }
}

#[test]
fn builds_current_read_only_local_storage_paths() {
    assert_eq!(
        super::local_storage_status("1", "2", "3"),
        "/api/v1/accounts/1/networks/2/sync_modules/3/local_storage/status"
    );
    assert_eq!(
        super::local_storage_manifest_request("1", "2", "3"),
        "/api/v1/accounts/1/networks/2/sync_modules/3/local_storage/manifest/request"
    );
    assert_eq!(
        super::local_storage_media("1", "2", "3", 4),
        "/api/v1/accounts/1/networks/2/sync_modules/3/local_storage/media/4"
    );
    assert_eq!(
        super::local_storage_clip_request("1", "2", "3", 4, 5),
        "/api/v1/accounts/1/networks/2/sync_modules/3/local_storage/manifest/4/clip/request/5"
    );
}

#[test]
fn builds_reviewed_destructive_local_storage_paths() {
    assert_eq!(
        super::local_storage_clip_delete("1", "2", "3", 4, 5),
        "/api/v1/accounts/1/networks/2/sync_modules/3/local_storage/manifest/4/clip/delete/5"
    );
    assert_eq!(
        super::local_storage_format("1", "2", "3"),
        "/api/v1/accounts/1/networks/2/sync_modules/3/local_storage/format"
    );
}

#[test]
fn provider_storage_contract_contains_only_reviewed_destructive_endpoints() {
    let source = concat!(
        include_str!("blink_api.rs"),
        include_str!("blink_storage.rs")
    );
    assert!(source.contains("clip/delete"));
    assert!(source.contains("local_storage/format"));
    assert!(!source.contains("local_storage/eject"));
    assert!(!source.contains("local_storage/mount"));
}

#[test]
fn preserves_device_specific_vendor_routes() {
    assert!(
        camera_action(&camera("default"), "9", &CameraAction::Live)
            .path
            .contains("/api/v6/")
    );
    assert!(
        camera_action(&camera("mini"), "9", &CameraAction::Live)
            .path
            .contains("/owls/")
    );
    assert!(
        camera_action(&camera("doorbell"), "9", &CameraAction::Record)
            .path
            .contains("/doorbells/")
    );
    assert!(
        camera_action(&camera("mini"), "9", &CameraAction::Motion(true))
            .body
            .is_some()
    );
    assert_eq!(
        live_command("9", "1", 7),
        "/accounts/9/networks/1/commands/7"
    );
    assert_eq!(
        camera_zones(&camera("default"), "9"),
        "/api/v2/accounts/9/networks/1/cameras/2/zones"
    );
    assert_eq!(
        camera_zones(&camera("mini"), "9"),
        "/api/v2/accounts/9/networks/1/owls/2/zones"
    );
    assert_eq!(
        camera_legacy_zones(&camera("default"), "9"),
        "/api/v1/accounts/9/networks/1/cameras/2/zones"
    );
    assert_eq!(
        temperature_alert(&camera("default"), "9", true),
        "/api/v1/accounts/9/networks/1/cameras/2/temp_alert_enable"
    );
}
