use crate::blink_client::BlinkError;
use serde_json::Value;

use crate::blink_settings::{SettingField, SettingKind};

pub fn encode_vendor_value(
    key: &str,
    current: &Value,
    desired: &Value,
) -> Result<Value, BlinkError> {
    if key == "ir_intensity" {
        return Ok(Value::from(match desired.as_str() {
            Some("low") => 1,
            Some("medium") => 4,
            Some("high") => 7,
            _ => return Err(BlinkError::InvalidSetting),
        }));
    }
    if key == "night_vision" {
        let selected = desired.as_str().ok_or(BlinkError::InvalidSetting)?;
        return if current.is_string() {
            Ok(Value::from(selected))
        } else {
            Ok(Value::from(match selected {
                "off" => 0,
                "on" => 1,
                "auto" => 2,
                _ => return Err(BlinkError::InvalidSetting),
            }))
        };
    }
    if current.is_i64() && desired.is_boolean() {
        return Ok(Value::from(i64::from(desired.as_bool() == Some(true))));
    }
    Ok(desired.clone())
}

pub fn add_bool(
    fields: &mut Vec<SettingField>,
    source: &Value,
    key: &str,
    vendor: &str,
    writable: bool,
) {
    if let Some(value) = bool_value(source.get(vendor)) {
        fields.push(field(
            key,
            Value::Bool(value),
            SettingKind::Boolean,
            writable,
        ));
    }
}

pub fn add_integer(
    fields: &mut Vec<SettingField>,
    source: &Value,
    key: &str,
    vendor: &str,
    range: (i64, i64, i64),
    writable: bool,
) {
    if let Some(value) = integer_value(source.get(vendor)) {
        let mut item = field(key, Value::from(value), SettingKind::Integer, writable);
        (item.min, item.max, item.step) = (Some(range.0), Some(range.1), Some(range.2));
        fields.push(item);
    }
}

pub fn add_select(
    fields: &mut Vec<SettingField>,
    key: &str,
    current: &str,
    options: &[&str],
    writable: bool,
) {
    if !options.contains(&current) {
        return;
    }
    let mut item = field(key, Value::from(current), SettingKind::Select, writable);
    item.options = options.iter().map(ToString::to_string).collect();
    fields.push(item);
}

pub fn add_text(
    fields: &mut Vec<SettingField>,
    source: &Value,
    key: &str,
    vendor: &str,
    max_length: i64,
    writable: bool,
) {
    let Some(value) = source.get(vendor).and_then(Value::as_str) else {
        return;
    };
    let mut item = field(key, Value::from(value), SettingKind::Text, writable);
    item.max = Some(max_length);
    fields.push(item);
}

pub fn bool_value(value: Option<&Value>) -> Option<bool> {
    value.and_then(|item| {
        item.as_bool()
            .or_else(|| item.as_i64().map(|number| number != 0))
    })
}

pub fn integer_value(value: Option<&Value>) -> Option<i64> {
    value.and_then(|item| {
        item.as_i64()
            .or_else(|| {
                let number = item.as_f64()?;
                if !number.is_finite() || number.fract() != 0.0 {
                    return None;
                }
                format!("{number:.0}").parse().ok()
            })
            .or_else(|| item.as_str()?.parse().ok())
    })
}

fn field(key: &str, value: Value, kind: SettingKind, writable: bool) -> SettingField {
    SettingField {
        key: key.into(),
        value,
        kind,
        writable,
        min: None,
        max: None,
        step: None,
        options: Vec::new(),
    }
}
