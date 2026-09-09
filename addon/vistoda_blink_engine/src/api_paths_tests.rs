use super::{
    CameraAction, camera_action, camera_legacy_zones, camera_zones, live_command, temperature_alert,
};
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
    }
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
