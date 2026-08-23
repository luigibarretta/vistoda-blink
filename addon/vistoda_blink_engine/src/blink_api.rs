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
    format!("/api/v3/accounts/{account}/homescreen")
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

const fn default_poll_seconds() -> f64 {
    1.0
}

#[cfg(test)]
mod tests {
    use super::{CameraAction, camera_action, live_command};
    use crate::blink_model::CameraState;

    fn camera(kind: &str) -> CameraState {
        CameraState {
            id: "2".into(),
            network_id: "1".into(),
            alias: "x".into(),
            name: "X".into(),
            serial: None,
            firmware: None,
            camera_type: kind.into(),
            product_type: kind.into(),
            enabled: None,
            status: None,
            battery_state: None,
            battery_voltage: None,
            battery_level: None,
            low_battery: None,
            temperature_f: None,
            wifi_dbm: None,
            motion_detected: false,
            thumbnail_url: None,
            powered: false,
        }
    }

    #[test]
    fn preserves_device_specific_vendor_routes() {
        assert!(
            camera_action(&camera("default"), "9", &CameraAction::Live)
                .path
                .contains("/api/v6/")
        );
        assert!(
            camera_action(&camera("mini"), "9", &CameraAction::Live)
                .path
                .contains("/owls/")
        );
        assert!(
            camera_action(&camera("doorbell"), "9", &CameraAction::Record)
                .path
                .contains("/doorbells/")
        );
        assert!(
            camera_action(&camera("mini"), "9", &CameraAction::Motion(true))
                .body
                .is_some()
        );
        assert_eq!(
            live_command("9", "1", 7),
            "/accounts/9/networks/1/commands/7"
        );
    }
}
