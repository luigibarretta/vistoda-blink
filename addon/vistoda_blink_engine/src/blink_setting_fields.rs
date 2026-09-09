use serde_json::Value;

use crate::blink_settings::{SettingField, SettingKind};

pub fn settings_fields(source: &Value, mutable: bool) -> Vec<SettingField> {
    let mut fields = Vec::new();
    recording_fields(&mut fields, source, mutable);
    motion_fields(&mut fields, source, mutable);
    diagnostic_fields(&mut fields, source);
    fields
}

fn recording_fields(fields: &mut Vec<SettingField>, source: &Value, mutable: bool) {
    add_bool(fields, source, "motion_detection", "enabled", mutable);
    add_bool(
        fields,
        source,
        "video_recording",
        "video_recording_enable",
        mutable && bool_value(source.get("video_recording_optional")) != Some(false),
    );
    add_bool(
        fields,
        source,
        "audio_streaming",
        "record_audio_enable",
        mutable && bool_value(source.get("record_audio")) != Some(false),
    );
    add_integer(
        fields,
        source,
        "clip_length",
        "video_length",
        (5, 60, 5),
        mutable,
    );
    add_quality(fields, source, mutable);
    add_bool(
        fields,
        source,
        "end_clip_early",
        "early_termination",
        mutable && bool_value(source.get("early_termination_supported")) != Some(false),
    );
    add_night_vision(fields, source, mutable);
    add_integer(
        fields,
        source,
        "ir_intensity",
        "illuminator_intensity",
        (1, 10, 1),
        false,
    );
}

fn motion_fields(fields: &mut Vec<SettingField>, source: &Value, mutable: bool) {
    add_integer(
        fields,
        source,
        "motion_sensitivity",
        "motion_sensitivity",
        (1, 9, 1),
        mutable,
    );
    add_integer(
        fields,
        source,
        "retrigger_time",
        "alert_interval",
        (10, 60, 10),
        mutable,
    );
    add_bool(
        fields,
        source,
        "early_notification",
        "early_notification",
        mutable && bool_value(source.get("early_notification_compatible")) != Some(false),
    );
}

fn diagnostic_fields(fields: &mut Vec<SettingField>, source: &Value) {
    add_bool(
        fields,
        source,
        "temperature_alerts",
        "temp_alarm_enable",
        false,
    );
    add_integer(
        fields,
        source,
        "temperature_min",
        "temp_min",
        (-40, 140, 1),
        false,
    );
    add_integer(
        fields,
        source,
        "temperature_max",
        "temp_max",
        (-40, 140, 1),
        false,
    );
}

fn add_bool(
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

fn add_integer(
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

fn add_quality(fields: &mut Vec<SettingField>, source: &Value, writable: bool) {
    let Some(current) = source.get("video_quality").and_then(Value::as_str) else {
        return;
    };
    let mut options = source
        .get("video_quality_support")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_else(|| vec!["saver".into(), "standard".into(), "best".into()]);
    if !options.iter().any(|item| item == current) {
        options.push(current.to_owned());
    }
    let mut item = field(
        "video_quality",
        Value::from(current),
        SettingKind::Select,
        writable,
    );
    item.options = options;
    fields.push(item);
}

fn add_night_vision(fields: &mut Vec<SettingField>, source: &Value, writable: bool) {
    let Some(value) = source.get("illuminator_enable") else {
        return;
    };
    let current = match value.as_str() {
        Some("off" | "on" | "auto") => value.as_str().map(str::to_owned),
        _ => integer_value(Some(value)).and_then(|item| match item {
            0 => Some("off".into()),
            1 => Some("on".into()),
            2 => Some("auto".into()),
            _ => None,
        }),
    };
    if let Some(current) = current {
        let mut item = field(
            "night_vision",
            Value::from(current),
            SettingKind::Select,
            writable,
        );
        item.options = vec!["off".into(), "on".into(), "auto".into()];
        fields.push(item);
    }
}

fn bool_value(value: Option<&Value>) -> Option<bool> {
    value.and_then(|item| {
        item.as_bool()
            .or_else(|| item.as_i64().map(|number| number != 0))
    })
}

fn integer_value(value: Option<&Value>) -> Option<i64> {
    value.and_then(|item| item.as_i64().or_else(|| item.as_str()?.parse().ok()))
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
