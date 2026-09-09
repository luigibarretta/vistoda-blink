use serde_json::Value;

use crate::{
    blink_model::CameraState,
    blink_setting_helpers::{
        add_bool, add_integer, add_select, add_text, bool_value, integer_value,
    },
    blink_settings::SettingField,
};

pub fn fields(result: &mut Vec<SettingField>, source: &Value, camera: &CameraState, mutable: bool) {
    add_text(result, source, "camera_name", "name", 255, mutable);
    let flip_compatible = bool_value(source.get("flip_video_compatible")) == Some(true);
    if flip_compatible {
        add_bool(result, source, "flip_video", "flip_video", mutable);
    }
    add_ir_intensity(result, source, mutable);
    add_status_led(result, source, camera, mutable);
    if camera.product_type == "catalina"
        && bool_value(source.get("snapshot_compatible")) == Some(true)
    {
        add_bool(result, source, "photo_capture", "snapshot_enabled", mutable);
    }
    if camera.product_type != "owl" {
        add_bool(
            result,
            source,
            "auto_thumbnail",
            "auto_update_thumbnail_enabled",
            false,
        );
    }
    if camera.camera_type == "mini" {
        add_integer(
            result,
            source,
            "speaker_volume",
            "volume_control",
            (1, 8, 1),
            mutable,
        );
    }
    add_integer(
        result,
        source,
        "sync_strength",
        "lfr_strength",
        (-100, 0, 1),
        false,
    );
    add_temperature(result, source, camera, mutable);
}

fn add_ir_intensity(fields: &mut Vec<SettingField>, source: &Value, writable: bool) {
    let current = match integer_value(source.get("illuminator_intensity")) {
        Some(1) => "low",
        Some(4) => "medium",
        Some(7) => "high",
        _ => return,
    };
    add_select(
        fields,
        "ir_intensity",
        current,
        &["low", "medium", "high"],
        writable,
    );
}

fn add_status_led(
    fields: &mut Vec<SettingField>,
    source: &Value,
    camera: &CameraState,
    writable: bool,
) {
    let Some(current) = source.get("led_state").and_then(Value::as_str) else {
        return;
    };
    let options = if camera.camera_type == "mini" {
        &["on", "off", "recording"][..]
    } else {
        &["off", "recording"][..]
    };
    add_select(fields, "status_led", current, options, writable);
}

fn add_temperature(
    fields: &mut Vec<SettingField>,
    source: &Value,
    camera: &CameraState,
    mutable: bool,
) {
    add_bool(
        fields,
        source,
        "temperature_alerts",
        "temp_alarm_enable",
        mutable && camera.camera_type == "default",
    );
    add_integer(
        fields,
        source,
        "temperature_min",
        "temp_min",
        (40, 90, 1),
        false,
    );
    add_integer(
        fields,
        source,
        "temperature_max",
        "temp_max",
        (40, 90, 1),
        false,
    );
}
