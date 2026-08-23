use std::{
    collections::{HashMap, HashSet},
    hash::BuildHasher,
};

use serde_json::Value;
use time::{OffsetDateTime, format_description::well_known::Iso8601};

use crate::blink_model::{CameraState, MediaClip};

#[must_use]
pub fn cameras<S1: BuildHasher, S2: BuildHasher>(
    account_id: &str,
    base_url: &str,
    usage: &Value,
    homescreen: &Value,
    details: &HashMap<String, Value, S1>,
    signals: &HashMap<String, Value, S2>,
    clips: &[MediaClip],
) -> Vec<CameraState> {
    let mut raw = Vec::new();
    for network in array(usage, "networks") {
        let network_id = text_or_number(network, "network_id").unwrap_or_default();
        for camera in array(network, "cameras") {
            raw.push((camera.clone(), network_id.clone(), "default"));
        }
    }
    for (key, camera_type) in [("owls", "mini"), ("doorbells", "doorbell")] {
        for camera in array(homescreen, key) {
            raw.push((
                camera.clone(),
                text_or_number(camera, "network_id").unwrap_or_default(),
                camera_type,
            ));
        }
    }
    let mut aliases = HashSet::new();
    raw.into_iter()
        .filter_map(|(summary, network_id, camera_type)| {
            let id = text_or_number(&summary, "id")?;
            let detail = details.get(&id).unwrap_or(&summary);
            let signal = signals.get(&id);
            let name = text(detail, "name")
                .or_else(|| text(&summary, "name"))
                .unwrap_or("Blink camera");
            Some(camera(
                detail,
                signal,
                clips,
                CameraContext {
                    account_id,
                    base_url,
                    id,
                    network_id,
                    alias: unique_alias(name, &mut aliases),
                    name: name.to_owned(),
                    camera_type,
                },
            ))
        })
        .collect()
}

struct CameraContext<'a> {
    account_id: &'a str,
    base_url: &'a str,
    id: String,
    network_id: String,
    alias: String,
    name: String,
    camera_type: &'a str,
}

fn camera(
    source: &Value,
    signal: Option<&Value>,
    clips: &[MediaClip],
    context: CameraContext<'_>,
) -> CameraState {
    let product_type = text(source, "type")
        .unwrap_or(context.camera_type)
        .to_owned();
    let signal = signal.unwrap_or_else(|| source.get("signals").unwrap_or(&Value::Null));
    let battery_state =
        owned_text(source, "battery_state").or_else(|| owned_text(source, "battery"));
    let thumbnail_url = thumbnail(source, &context, &product_type);
    CameraState {
        id: context.id,
        network_id: context.network_id,
        alias: context.alias,
        name: context.name.clone(),
        serial: owned_text(source, "serial"),
        firmware: owned_text(source, "fw_version"),
        camera_type: context.camera_type.to_owned(),
        product_type: product_type.clone(),
        enabled: boolean(source, "enabled"),
        status: owned_text(source, "status"),
        battery_state: battery_state.clone(),
        battery_voltage: unsigned(source, "battery_voltage"),
        battery_level: unsigned(signal, "battery"),
        low_battery: battery_state.as_deref().map(|value| value != "ok"),
        temperature_f: number(signal, "temp").or_else(|| number(source, "temperature")),
        wifi_dbm: integer(source, "wifi_strength"),
        motion_detected: clips
            .iter()
            .any(|clip| clip.camera_name == context.name && recent(&clip.created_at)),
        thumbnail_url,
        powered: context.camera_type == "mini" || product_type == "owl",
    }
}

fn thumbnail(source: &Value, context: &CameraContext<'_>, product_type: &str) -> Option<String> {
    let value = text_or_number(source, "thumbnail")?;
    if value.starts_with("http") {
        return Some(if value.ends_with("&ext=") {
            value
        } else {
            format!("{value}.jpg")
        });
    }
    Some(format!(
        "{}/api/v3/media/accounts/{}/networks/{}/{}/{}/thumbnail/thumbnail.jpg?ts={value}&ext=",
        context.base_url, context.account_id, context.network_id, product_type, context.id
    ))
}

#[must_use]
pub fn media(value: &Value) -> Vec<MediaClip> {
    array(value, "media")
        .iter()
        .filter_map(|item| {
            let camera_name = text(item, "device_name")?.to_owned();
            let created_at = text(item, "created_at")?.to_owned();
            Some(MediaClip {
                id: text_or_number(item, "id")
                    .unwrap_or_else(|| format!("{camera_name}:{created_at}")),
                camera_id: text_or_number(item, "device_id"),
                camera_name,
                created_at,
                media_url: text(item, "media")?.to_owned(),
                deleted: boolean(item, "deleted").unwrap_or(false),
            })
        })
        .collect()
}

fn unique_alias(name: &str, used: &mut HashSet<String>) -> String {
    let base = name
        .to_ascii_lowercase()
        .chars()
        .map(|value| {
            if value.is_ascii_alphanumeric() {
                value
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_owned();
    let base = if base.is_empty() { "camera" } else { &base };
    let mut candidate = base.to_owned();
    let mut suffix = 2;
    while !used.insert(candidate.clone()) {
        candidate = format!("{base}_{suffix}");
        suffix += 1;
    }
    candidate
}

fn recent(value: &str) -> bool {
    OffsetDateTime::parse(value, &Iso8601::DEFAULT)
        .is_ok_and(|created| OffsetDateTime::now_utc() - created <= time::Duration::minutes(2))
}

fn array<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}
fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key)?.as_str()
}
fn owned_text(value: &Value, key: &str) -> Option<String> {
    text(value, key).map(ToOwned::to_owned)
}
fn boolean(value: &Value, key: &str) -> Option<bool> {
    value.get(key)?.as_bool()
}
fn unsigned(value: &Value, key: &str) -> Option<u64> {
    value.get(key)?.as_u64()
}
fn integer(value: &Value, key: &str) -> Option<i64> {
    value.get(key)?.as_i64()
}
fn number(value: &Value, key: &str) -> Option<f64> {
    value.get(key)?.as_f64()
}
fn text_or_number(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(|item| {
        item.as_str()
            .map(ToOwned::to_owned)
            .or_else(|| item.as_u64().map(|number| number.to_string()))
    })
}
