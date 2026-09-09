use serde_json::json;

use super::{CapabilityField, collect_fields, feature_values};

#[test]
fn reports_only_names_and_types_with_bounded_paths() {
    let source = json!({
        "enabled": true,
        "quality": "best",
        "limits": {"clip": 60},
        "secret": "must-never-be-returned"
    });
    let mut fields = Vec::new();
    collect_fields(&source, "", 0, &mut fields);
    assert!(fields.contains(&CapabilityField {
        path: "enabled".into(),
        kind: "boolean"
    }));
    assert!(fields.contains(&CapabilityField {
        path: "limits.clip".into(),
        kind: "integer"
    }));
    let serialized = serde_json::to_string(&fields).unwrap_or_default();
    assert!(!serialized.contains("must-never-be-returned"));
    assert!(!serialized.contains("best"));
    assert!(feature_values(&source).is_empty());
}

#[test]
fn returns_only_explicit_non_secret_feature_values() {
    let source = json!({
        "led_state": "off",
        "snapshot_enabled": true,
        "snapshot_period_minutes_options": [60, 120],
        "serial": "must-never-be-returned"
    });
    let features = feature_values(&source);
    let serialized = serde_json::to_string(&features).unwrap_or_default();
    assert_eq!(features.get("snapshot_enabled"), Some(&json!(true)));
    assert!(serialized.contains("led_state"));
    assert!(!serialized.contains("must-never-be-returned"));
}
