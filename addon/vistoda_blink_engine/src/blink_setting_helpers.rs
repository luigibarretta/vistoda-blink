use serde_json::Value;

use crate::blink_settings::{SettingField, SettingKind};

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
