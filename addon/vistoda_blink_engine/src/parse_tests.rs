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
    let home = json!({"owls":[{"id":284_471,"network_id":7,"name":"Cucina",
        "serial":"G8T1940003110MK9","type":"owl","enabled":true,"status":"online",
        "thumbnail":"/api/v3/media/accounts/85085/networks/85507/owl/284471/thumbnail/thumbnail.jpg?ts=1789316623&ext=",
        "signals":{"temp":79},"wifi_strength":-51}]});
    let details = HashMap::from([(
        "284471".to_owned(),
        json!({"id":284_471,"name":"Cucina","type":"owl"}),
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
            .is_some_and(|url| url == "https://rest-prod.immedia-semi.com/api/v3/media/accounts/85085/networks/85507/owl/284471/thumbnail/thumbnail.jpg?ts=1789316623&ext=")
    );
}

#[test]
fn exposes_camera_connectivity_only_for_known_status_values() {
    let usage = json!({"networks":[{"network_id":7,"cameras":[
        {"id":1,"name":"Balcone","status":"done"},
        {"id":2,"name":"Garage","status":"offline"},
        {"id":3,"name":"Cortile","status":"busy"},
        {"id":4,"name":"Ingresso"}]}]});
    let result = cameras(
        "42",
        "https://rest-prod.immedia-semi.com",
        &usage,
        &json!({}),
        &HashMap::new(),
        &HashMap::new(),
        &[],
    );
    let online = result
        .iter()
        .map(|camera| camera.online)
        .collect::<Vec<_>>();
    assert_eq!(online, [Some(true), Some(false), None, None]);
    assert_eq!(result[2].status.as_deref(), Some("busy"));
    let value = serde_json::to_value(&result[1]).unwrap_or_default();
    assert_eq!(value["online"], false);
}

#[test]
fn sync_module_status_drives_network_connectivity() {
    let catalog = json!({"summary":{
        "7":{"id":7,"name":"Casa","onboarded":true},
        "8":{"id":8,"name":"Garage","onboarded":true}}});
    let home = json!({
        "sync_modules": [{"id":77,"network_id":7,"status":"offline"}],
        "owls": [{"id":3,"network_id":9,"name":"Mini","onboarded":true,"status":"online"}]
    });
    let updates = HashMap::from([(
        "8".to_owned(),
        json!({"network":{"name":"Garage","status":"ok"},
            "_vistoda_sync":{"syncmodule":{"id":88,"status":"online"}}}),
    )]);
    let result = networks(&catalog, &home, &updates);
    let casa = result.iter().find(|network| network.id == "7");
    let garage = result.iter().find(|network| network.id == "8");
    let mini = result.iter().find(|network| network.id == "9");
    assert_eq!(
        casa.map(|item| (item.online, item.has_sync_module)),
        Some((Some(false), true))
    );
    assert_eq!(garage.and_then(|item| item.online), Some(true));
    assert_eq!(garage.and_then(|item| item.status.as_deref()), Some("ok"));
    assert_eq!(
        mini.map(|item| (item.online, item.has_sync_module)),
        Some((Some(true), false))
    );
    let value = serde_json::to_value(casa).unwrap_or_default();
    assert_eq!(value["has_sync_module"], true);
    assert!(value.get("sync_module_id").is_none());
}
