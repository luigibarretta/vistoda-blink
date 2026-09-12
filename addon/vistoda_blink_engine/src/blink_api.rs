use serde::{Deserialize, Serialize};

use crate::blink_model::CameraState;

pub const TIER_URL: &str = "https://rest-prod.immedia-semi.com/api/v1/users/tier_info";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TierInfo {
    pub tier: String,
    pub account_id: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Command {
    #[serde(alias = "id")]
    pub command_id: u64,
    pub network_id: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LiveDescriptor {
    pub server: String,
    pub command_id: u64,
    #[serde(default = "default_poll_seconds")]
    pub polling_interval: f64,
    #[serde(default, deserialize_with = "optional_bool")]
    pub is_multi_client_live_view: Option<bool>,
}

fn optional_bool<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<bool>, D::Error> {
    Ok(serde_json::Value::deserialize(deserializer)?.as_bool())
}

pub enum CameraAction {
    Motion(bool),
    Record,
    Snapshot,
    Live,
}

pub struct RequestSpec {
    pub path: String,
    pub body: Option<serde_json::Value>,
}

pub fn base_url(tier: &str) -> String {
    format!("https://rest-{tier}.immedia-semi.com")
}

pub fn homescreen(account: &str) -> String {
    format!("/api/v4/accounts/{account}/homescreen")
}

pub const fn networks() -> &'static str {
    "/networks"
}

pub fn network_update(network: &str) -> String {
    format!("/network/{network}/update")
}

pub fn media(account: &str, since: &str, page: u8) -> String {
    format!("/api/v1/accounts/{account}/media/changed?since={since}&page={page}")
}

pub fn camera_config(camera: &CameraState, account: &str) -> String {
    match camera.camera_type.as_str() {
        "mini" => format!(
            "/api/v1/accounts/{account}/networks/{}/owls/{}/config",
            camera.network_id, camera.id
        ),
        _ => format!("/network/{}/camera/{}/config", camera.network_id, camera.id),
    }
}

pub fn camera_update(camera: &CameraState, account: &str) -> String {
    match camera.camera_type.as_str() {
        "mini" => camera_config(camera, account),
        _ => format!("/network/{}/camera/{}/update", camera.network_id, camera.id),
    }
}

pub fn camera_zones(camera: &CameraState, account: &str) -> String {
    let device = if camera.camera_type == "mini" {
        "owls"
    } else {
        "cameras"
    };
    format!(
        "/api/v2/accounts/{account}/networks/{}/{device}/{}/zones",
        camera.network_id, camera.id
    )
}

pub fn camera_legacy_zones(camera: &CameraState, account: &str) -> String {
    format!(
        "/api/v1/accounts/{account}/networks/{}/cameras/{}/zones",
        camera.network_id, camera.id
    )
}

pub fn temperature_alert(camera: &CameraState, account: &str, enabled: bool) -> String {
    let action = if enabled {
        "temp_alert_enable"
    } else {
        "temp_alert_disable"
    };
    format!(
        "/api/v1/accounts/{account}/networks/{}/cameras/{}/{action}",
        camera.network_id, camera.id
    )
}

pub fn camera_action(camera: &CameraState, account: &str, action: &CameraAction) -> RequestSpec {
    let (name, body) = match action {
        CameraAction::Motion(enabled) => (
            if *enabled { "enable" } else { "disable" },
            (camera.camera_type == "mini").then(|| serde_json::json!({"enabled": *enabled})),
        ),
        CameraAction::Record => ("clip", None),
        CameraAction::Snapshot => ("thumbnail", None),
        CameraAction::Live => (
            "liveview",
            Some(serde_json::json!({
                "intent": "liveview",
                "motion_event_start_time": null
            })),
        ),
    };
    let path = match (&*camera.camera_type, action) {
        ("mini", CameraAction::Live) => format!(
            "/api/v2/accounts/{account}/networks/{}/owls/{}/liveview",
            camera.network_id, camera.id
        ),
        ("doorbell", CameraAction::Live) => format!(
            "/api/v2/accounts/{account}/networks/{}/doorbells/{}/liveview",
            camera.network_id, camera.id
        ),
        (_, CameraAction::Live) => format!(
            "/api/v6/accounts/{account}/networks/{}/cameras/{}/liveview",
            camera.network_id, camera.id
        ),
        ("mini", CameraAction::Motion(_)) => format!(
            "/api/v1/accounts/{account}/networks/{}/owls/{}/config",
            camera.network_id, camera.id
        ),
        ("mini", _) => format!(
            "/api/v1/accounts/{account}/networks/{}/owls/{}/{name}",
            camera.network_id, camera.id
        ),
        ("doorbell", _) => format!(
            "/api/v1/accounts/{account}/networks/{}/doorbells/{}/{name}",
            camera.network_id, camera.id
        ),
        _ => format!("/network/{}/camera/{}/{name}", camera.network_id, camera.id),
    };
    RequestSpec { path, body }
}

pub fn arm(account: &str, network: &str, armed: bool) -> String {
    let action = if armed { "arm" } else { "disarm" };
    format!("/api/v1/accounts/{account}/networks/{network}/state/{action}")
}

pub fn command(network: &str, id: u64) -> String {
    format!("/network/{network}/command/{id}")
}

pub fn command_done(network: &str, id: u64) -> String {
    format!("/network/{network}/command/{id}/done/")
}

pub fn live_command(account: &str, network: &str, id: u64) -> String {
    format!("/accounts/{account}/networks/{network}/commands/{id}")
}

pub fn live_command_done(account: &str, network: &str, id: u64) -> String {
    format!("/accounts/{account}/networks/{network}/commands/{id}/done")
}

pub fn current_command(account: &str, network: &str, id: u64) -> String {
    format!("/accounts/{account}/networks/{network}/commands/{id}")
}

pub fn local_storage_status(account: &str, network: &str, sync: &str) -> String {
    format!(
        "/api/v1/accounts/{account}/networks/{network}/sync_modules/{sync}/local_storage/status"
    )
}

pub fn local_storage_manifest_request(account: &str, network: &str, sync: &str) -> String {
    format!(
        "/api/v1/accounts/{account}/networks/{network}/sync_modules/{sync}/local_storage/manifest/request"
    )
}

pub fn local_storage_media(account: &str, network: &str, sync: &str, command: u64) -> String {
    format!(
        "/api/v1/accounts/{account}/networks/{network}/sync_modules/{sync}/local_storage/media/{command}"
    )
}

pub fn local_storage_clip_request(
    account: &str,
    network: &str,
    sync: &str,
    manifest: u64,
    clip: u64,
) -> String {
    format!(
        "/api/v1/accounts/{account}/networks/{network}/sync_modules/{sync}/local_storage/manifest/{manifest}/clip/request/{clip}"
    )
}

pub fn local_storage_clip_delete(
    account: &str,
    network: &str,
    sync: &str,
    manifest: u64,
    clip: u64,
) -> String {
    format!(
        "/api/v1/accounts/{account}/networks/{network}/sync_modules/{sync}/local_storage/manifest/{manifest}/clip/delete/{clip}"
    )
}

pub fn local_storage_format(account: &str, network: &str, sync: &str) -> String {
    format!(
        "/api/v1/accounts/{account}/networks/{network}/sync_modules/{sync}/local_storage/format"
    )
}

const fn default_poll_seconds() -> f64 {
    1.0
}

#[cfg(test)]
#[path = "api_paths_tests.rs"]
mod tests;
