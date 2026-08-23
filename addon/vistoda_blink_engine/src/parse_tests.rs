use std::collections::HashMap;

use serde_json::json;

use crate::blink_model::{cameras, media, networks};

#[test]
fn parses_official_surface_without_vendor_types() {
    let home = json!({"networks":[{"id":7,"name":"Casa"}],"owls":[{"id":2,"network_id":7,"name":"Kitchen","type":"owl","enabled":true,"signals":{"battery":3,"temp":72}}]});
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
    assert_eq!(networks(&catalog, &home, &HashMap::new())[0].id, "7");
    assert_eq!(cameras.len(), 2);
    assert_eq!(cameras[0].alias, "kitchen");
    assert_eq!(cameras[1].alias, "kitchen_2");
    assert!(cameras[1].powered);
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
