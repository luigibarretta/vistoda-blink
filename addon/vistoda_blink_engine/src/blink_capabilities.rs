use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

use crate::{
    blink_client::{BlinkClient, BlinkError},
    blink_settings,
};

const MAX_FIELDS: usize = 160;
const MAX_DEPTH: usize = 4;
const MAX_PATH_LEN: usize = 128;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CapabilityField {
    pub path: String,
    pub kind: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct CameraCapabilities {
    pub alias: String,
    pub camera_type: String,
    pub fields: Vec<CapabilityField>,
    pub features: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ZoneCapabilities {
    pub alias: String,
    pub camera_type: String,
    pub fields: Vec<CapabilityField>,
}

impl BlinkClient {
    pub async fn camera_capabilities(&self, alias: &str) -> Result<CameraCapabilities, BlinkError> {
        let (camera, _, response) = self.read_settings(alias).await?;
        let mut fields = Vec::new();
        collect_fields(blink_settings::config_object(&response), "", 0, &mut fields);
        fields.sort_by(|left, right| left.path.cmp(&right.path));
        fields.dedup();
        Ok(CameraCapabilities {
            alias: camera.alias,
            camera_type: camera.camera_type,
            fields,
            features: feature_values(blink_settings::config_object(&response)),
        })
    }

    pub async fn zone_capabilities(&self, alias: &str) -> Result<ZoneCapabilities, BlinkError> {
        let camera = self.camera(alias).await?;
        let context = self.context().await?;
        let response = self
            .get_json(
                &context,
                &crate::blink_api::camera_zones(&camera, &context.account_id),
            )
            .await?;
        let mut fields = Vec::new();
        collect_fields(&response, "", 0, &mut fields);
        fields.sort_by(|left, right| left.path.cmp(&right.path));
        fields.dedup();
        Ok(ZoneCapabilities {
            alias: camera.alias,
            camera_type: camera.camera_type,
            fields,
        })
    }
}

fn collect_fields(value: &Value, prefix: &str, depth: usize, fields: &mut Vec<CapabilityField>) {
    if depth >= MAX_DEPTH || fields.len() >= MAX_FIELDS {
        return;
    }
    let Some(object) = value.as_object() else {
        return;
    };
    for (key, child) in object {
        if fields.len() >= MAX_FIELDS {
            break;
        }
        if !safe_key(key) {
            continue;
        }
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        if path.len() > MAX_PATH_LEN {
            continue;
        }
        fields.push(CapabilityField {
            path: path.clone(),
            kind: value_kind(child),
        });
        collect_value(child, &path, depth + 1, fields);
    }
}

fn collect_value(value: &Value, prefix: &str, depth: usize, fields: &mut Vec<CapabilityField>) {
    if value.is_object() {
        collect_fields(value, prefix, depth, fields);
    } else if let Some(first) = value.as_array().and_then(|items| items.first()) {
        let path = format!("{prefix}[]");
        if path.len() <= MAX_PATH_LEN && fields.len() < MAX_FIELDS {
            fields.push(CapabilityField {
                path: path.clone(),
                kind: value_kind(first),
            });
            collect_value(first, &path, depth, fields);
        }
    }
}

fn feature_values(source: &Value) -> BTreeMap<String, Value> {
    const KEYS: &[&str] = &[
        "auto_update_thumbnail_enabled",
        "clip_length",
        "clip_length_max",
        "flip_video",
        "flip_video_compatible",
        "illuminator_intensity",
        "led_enabled",
        "led_state",
        "lfr_strength",
        "motion_regions_compatible",
        "privacy_zones_compatible",
        "retrigger_time",
        "snapshot_compatible",
        "snapshot_enabled",
        "snapshot_period_minutes",
        "snapshot_period_minutes_options",
        "temp_alarm_enable",
        "temp_max",
        "temp_min",
        "volume_control",
        "zone_version",
    ];
    KEYS.iter()
        .filter_map(|key| {
            safe_feature_value(source.get(key)?).map(|value| ((*key).to_owned(), value))
        })
        .collect()
}

fn safe_feature_value(value: &Value) -> Option<Value> {
    match value {
        Value::Bool(_) | Value::Number(_) => Some(value.clone()),
        Value::String(text) if text.len() <= 64 => Some(value.clone()),
        Value::Array(items)
            if items.len() <= 16
                && items.iter().all(|item| {
                    matches!(item, Value::Bool(_) | Value::Number(_))
                        || item.as_str().is_some_and(|text| text.len() <= 64)
                }) =>
        {
            Some(value.clone())
        }
        _ => None,
    }
}

fn safe_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 64
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(number) if number.is_i64() || number.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
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
}
