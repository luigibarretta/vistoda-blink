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
        collect_fields(child, &path, depth + 1, fields);
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

    use super::{CapabilityField, collect_fields};

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
    }
}
