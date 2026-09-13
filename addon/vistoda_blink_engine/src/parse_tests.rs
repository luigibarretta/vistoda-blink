use std::collections::HashMap;

use serde_json::json;

use crate::blink_model::{cameras, media, networks};

#[test]
fn parses_official_surface_without_vendor_types() {
    let home = json!({"networks":[{"id":7,"name":"Casa"}],
        "sync_modules":[{"id":77,"network_id":7}],
        "owls":[{"id":2,"network_id":7,"name":"Kitchen","type":"owl","enabled":true,
        "ring_device_id":"22","two_way_audio":true,"audio_aec":true,
        "signals":{"battery":3,"temp":72}}]});
    let usage = json!({"networks":[{"network_id":7,"cameras":[{"id":1,"name":"Kitchen"}]}]});
    let clips = media(
        &json!({"media":[{"id":9,"device_name":"Kitchen","created_at":"2026-01-01T00:00:00Z","media":"/clip.mp4"}]}),
    );
    let cameras = cameras(
        "42",
        "https://rest-prod.immedia-semi.com",
        &usage,
        &home,
        &HashMap::new(),
        &HashMap::new(),
        &clips,
    );
    let catalog = json!({"summary":{"7":{"id":7,"name":"Casa","onboarded":true}}});
    let networks = networks(&catalog, &home, &HashMap::new());
    assert_eq!(networks[0].id, "7");
    assert_eq!(networks[0].sync_module_id.as_deref(), Some("77"));
    assert_eq!(cameras.len(), 2);
    assert_eq!(cameras[0].alias, "kitchen");
    assert_eq!(cameras[1].alias, "kitchen_2");
    assert!(cameras[1].powered);
    assert_eq!(cameras[1].ring_device_id, Some(22));
    assert_eq!(cameras[1].two_way_audio, Some(true));
    assert_eq!(
        serde_json::to_value(&cameras[1]).unwrap_or_default()["preferred_live_transport"],
        "walnut"
    );
    assert!(
        !serde_json::to_string(&cameras[1])
            .unwrap_or_default()
            .contains("ring_device_id")
    );
}

#[test]
fn adds_only_onboarded_sync_less_networks_once() {
    let catalog = json!({"summary":{"7":{"id":7,"name":"Casa","onboarded":true}}});
    let home = json!({
        "networks": [{"id": 7, "name": "Casa"}],
        "owls": [
            {"id": 2, "network_id": 7, "name": "Attached", "onboarded": true},
            {"id": 3, "network_id": 9, "name": "Standalone", "onboarded": true},
            {"id": 4, "network_id": 10, "name": "Pending", "onboarded": false}
        ]
    });
    let result = networks(&catalog, &home, &HashMap::new());
    assert_eq!(result.len(), 2);
    assert_eq!(result[1].id, "9");
}

#[test]
fn derives_temperature_alert_state_from_the_same_camera_configuration() {
    let usage = json!({"networks":[{"network_id":7,"cameras":[{"id":1,"name":"Balcone"}]}]});
    let details = HashMap::from([(
        "1".to_owned(),
        json!({"id":1,"name":"Balcone","serial":"ABC","temp_alarm_enable":true,
            "temp_min":39,"temp_max":90}),
    )]);
    let signals = HashMap::from([("1".to_owned(), json!({"temp":94}))]);
    let result = cameras(
        "42",
        "https://rest-prod.immedia-semi.com",
        &usage,
        &json!({}),
        &details,
        &signals,
        &[],
    );
    assert_eq!(result[0].temperature_alerts, Some(true));
    assert_eq!(result[0].temperature_min_f, Some(39));
    assert_eq!(result[0].temperature_max_f, Some(90));
    assert_eq!(result[0].temperature_out_of_range, Some(true));
}

#[test]
fn preserves_homescreen_identity_and_snapshot_when_config_is_sparse() {
    let home = json!({"owls":[{"id":284471,"network_id":7,"name":"Cucina",
        "serial":"G8T1940003110MK9","type":"owl","enabled":true,"status":"online",
        "thumbnail":"1700000000","signals":{"temp":79},"wifi_strength":-51}]});
    let details = HashMap::from([(
        "284471".to_owned(),
        json!({"id":284471,"name":"Cucina","type":"owl"}),
    )]);
    let result = cameras(
        "42",
        "https://rest-prod.immedia-semi.com",
        &json!({}),
        &home,
        &details,
        &HashMap::new(),
        &[],
    );
    let camera = &result[0];
    assert_eq!(camera.serial.as_deref(), Some("G8T1940003110MK9"));
    assert_eq!(camera.temperature_f, Some(79.0));
    assert_eq!(camera.wifi_dbm, Some(-51));
    assert_eq!(camera.status.as_deref(), Some("online"));
    assert!(
        camera
            .thumbnail_url
            .as_deref()
            .is_some_and(|url| url.contains("1700000000"))
    );
}
