use std::fmt::Write;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{blink_model::CameraState, blink_setting_fields::settings_fields};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingKind {
    Boolean,
    Integer,
    Select,
    Text,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SettingField {
    pub key: String,
    pub value: Value,
    pub kind: SettingKind,
    pub writable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<i64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CameraSettings {
    pub alias: String,
    pub name: String,
    pub camera_type: String,
    pub product_type: String,
    pub firmware: Option<String>,
    pub battery_state: Option<String>,
    pub temperature_f: Option<f64>,
    pub wifi_dbm: Option<i64>,
    pub revision: String,
    pub settings: Vec<SettingField>,
}

pub(crate) fn parse(camera: &CameraState, response: &Value) -> CameraSettings {
    let config = config_object(response);
    let mutable = matches!(camera.camera_type.as_str(), "default" | "mini");
    let settings = settings_fields(config, camera, mutable);
    let revision = revision(&settings);
    CameraSettings {
        alias: camera.alias.clone(),
        name: config
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(&camera.name)
            .to_owned(),
        camera_type: camera.camera_type.clone(),
        product_type: camera.product_type.clone(),
        firmware: camera.firmware.clone(),
        battery_state: camera.battery_state.clone(),
        temperature_f: camera.temperature_f,
        wifi_dbm: camera.wifi_dbm,
        revision,
        settings,
    }
}

pub(crate) fn config_object(value: &Value) -> &Value {
    let camera = value.get("camera");
    camera
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .or_else(|| camera.filter(|item| item.is_object()))
        .unwrap_or(value)
}

fn revision(fields: &[SettingField]) -> String {
    let digest = Sha256::digest(serde_json::to_vec(fields).unwrap_or_default());
    digest
        .iter()
        .fold(String::with_capacity(64), |mut result, byte| {
            let _ = write!(result, "{byte:02x}");
            result
        })
}
