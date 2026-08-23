use serde::{Deserialize, Serialize};

pub use crate::{
    blink_network_parse::networks,
    blink_parse::{cameras, media},
};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ProviderState {
    pub account_id: String,
    pub updated_at: u64,
    pub networks: Vec<NetworkState>,
    pub cameras: Vec<CameraState>,
    pub clips: Vec<MediaClip>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NetworkState {
    pub id: String,
    pub name: String,
    pub armed: Option<bool>,
    pub status: Option<String>,
    pub serial: Option<String>,
    pub firmware: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CameraState {
    pub id: String,
    pub network_id: String,
    pub alias: String,
    pub name: String,
    pub serial: Option<String>,
    pub firmware: Option<String>,
    pub camera_type: String,
    pub product_type: String,
    pub enabled: Option<bool>,
    pub status: Option<String>,
    pub battery_state: Option<String>,
    pub battery_voltage: Option<u64>,
    pub battery_level: Option<u64>,
    pub low_battery: Option<bool>,
    pub temperature_f: Option<f64>,
    pub wifi_dbm: Option<i64>,
    pub motion_detected: bool,
    pub thumbnail_url: Option<String>,
    pub powered: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MediaClip {
    pub id: String,
    pub camera_id: Option<String>,
    pub camera_name: String,
    pub created_at: String,
    pub media_url: String,
    pub deleted: bool,
}
