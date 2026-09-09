use serde_json::Value;

use crate::{
    blink_model::CameraState,
    blink_setting_advanced,
    blink_setting_helpers::{add_bool, add_integer, add_select, bool_value, integer_value},
    blink_settings::SettingField,
};

pub fn settings_fields(source: &Value, camera: &CameraState, mutable: bool) -> Vec<SettingField> {
    let mut fields = Vec::new();
    recording_fields(&mut fields, source, camera, mutable);
    motion_fields(&mut fields, source, camera, mutable);
    blink_setting_advanced::fields(&mut fields, source, camera, mutable);
    fields
}

fn recording_fields(
    fields: &mut Vec<SettingField>,
    source: &Value,
    camera: &CameraState,
    mutable: bool,
) {
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
        if camera.camera_type == "mini" {
            "clip_length"
        } else {
            "video_length"
        },
        (
            5,
            integer_value(source.get("clip_length_max")).unwrap_or(60),
            5,
        ),
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
}

fn motion_fields(
    fields: &mut Vec<SettingField>,
    source: &Value,
    camera: &CameraState,
    mutable: bool,
) {
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
        if camera.camera_type == "mini" {
            "retrigger_time"
        } else {
            "alert_interval"
        },
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
    let options = options.iter().map(String::as_str).collect::<Vec<_>>();
    add_select(fields, "video_quality", current, &options, writable);
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
        add_select(
            fields,
            "night_vision",
            &current,
            &["off", "on", "auto"],
            writable,
        );
    }
}
