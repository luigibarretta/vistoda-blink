use std::{
    collections::{HashMap, HashSet},
    hash::BuildHasher,
};

use serde_json::Value;

use crate::blink_model::NetworkState;

#[must_use]
pub fn networks<S: BuildHasher>(
    catalog: &Value,
    homescreen: &Value,
    updates: &HashMap<String, Value, S>,
) -> Vec<NetworkState> {
    let mut result = catalog_networks(catalog, homescreen, updates);
    let mut ids = result
        .iter()
        .map(|network| network.id.clone())
        .collect::<HashSet<_>>();
    for key in ["owls", "doorbells"] {
        for device in array(homescreen, key) {
            let Some(id) = text_or_number(device, "network_id") else {
                continue;
            };
            if boolean(device, "onboarded") != Some(true) || !ids.insert(id.clone()) {
                continue;
            }
            result.push(NetworkState {
                id,
                name: text(device, "name").unwrap_or("Blink system").to_owned(),
                armed: boolean(device, "enabled"),
                status: owned_text(device, "status"),
                serial: owned_text(device, "serial"),
                firmware: owned_text(device, "fw_version"),
            });
        }
    }
    result
}

fn catalog_networks<S: BuildHasher>(
    catalog: &Value,
    homescreen: &Value,
    updates: &HashMap<String, Value, S>,
) -> Vec<NetworkState> {
    let fallback = array(homescreen, "networks")
        .iter()
        .filter_map(|item| text_or_number(item, "id").map(|id| (id, item)))
        .collect::<Vec<_>>();
    let summaries = catalog
        .get("summary")
        .and_then(Value::as_object)
        .map(|items| {
            items
                .iter()
                .filter(|(_, item)| boolean(item, "onboarded") != Some(false))
                .map(|(key, item)| {
                    (
                        text_or_number(item, "id").unwrap_or_else(|| key.clone()),
                        item,
                    )
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or(fallback);
    summaries
        .into_iter()
        .map(|(id, summary)| network(id, summary, updates))
        .collect()
}

fn network<S: BuildHasher>(
    id: String,
    summary: &Value,
    updates: &HashMap<String, Value, S>,
) -> NetworkState {
    let update = updates.get(&id);
    let source = update
        .and_then(|value| value.get("network"))
        .unwrap_or(summary);
    let module = update
        .and_then(|value| value.get("_vistoda_sync"))
        .and_then(|value| value.get("syncmodule"));
    NetworkState {
        id,
        name: text(source, "name").unwrap_or("Blink system").to_owned(),
        armed: boolean(source, "armed"),
        status: owned_text(source, "status")
            .or_else(|| module.and_then(|value| owned_text(value, "status"))),
        serial: owned_text(source, "serial")
            .or_else(|| module.and_then(|value| owned_text(value, "serial"))),
        firmware: owned_text(source, "fw_version")
            .or_else(|| module.and_then(|value| owned_text(value, "fw_version"))),
    }
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

fn text_or_number(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(|item| {
        item.as_str()
            .map(ToOwned::to_owned)
            .or_else(|| item.as_u64().map(|number| number.to_string()))
    })
}
