use std::{
    collections::{HashMap, HashSet},
    hash::BuildHasher,
};

use serde_json::Value;

use crate::{
    blink_connectivity::online,
    blink_json::{array, boolean, owned_text, text, text_or_number},
    blink_model::NetworkState,
};

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
            let status = owned_text(device, "status");
            result.push(NetworkState {
                id,
                name: text(device, "name").unwrap_or("Blink system").to_owned(),
                armed: boolean(device, "enabled"),
                online: online(status.as_deref()),
                has_sync_module: false,
                status,
                serial: owned_text(device, "serial"),
                firmware: owned_text(device, "fw_version"),
                sync_module_id: None,
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
    let modules = array(homescreen, "sync_modules")
        .iter()
        .filter_map(|item| text_or_number(item, "network_id").map(|id| (id, item)))
        .collect::<HashMap<_, _>>();
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
        .map(|(id, summary)| {
            let module = modules.get(&id).copied();
            network(id, summary, updates, module)
        })
        .collect()
}

fn network<S: BuildHasher>(
    id: String,
    summary: &Value,
    updates: &HashMap<String, Value, S>,
    homescreen_module: Option<&Value>,
) -> NetworkState {
    let update = updates.get(&id);
    let source = update
        .and_then(|value| value.get("network"))
        .unwrap_or(summary);
    let module = update
        .and_then(|value| value.get("_vistoda_sync"))
        .and_then(|value| value.get("syncmodule"))
        .and_then(|value| {
            value
                .as_array()
                .and_then(|items| items.first())
                .or(Some(value))
        })
        .or(homescreen_module);
    let module_status = module.and_then(|value| owned_text(value, "status"));
    let status = owned_text(source, "status").or_else(|| module_status.clone());
    let sync_module_id = module.and_then(|value| text_or_number(value, "id"));
    NetworkState {
        id,
        name: text(source, "name").unwrap_or("Blink system").to_owned(),
        armed: boolean(source, "armed"),
        // The Sync Module's own status is the authoritative connectivity
        // signal; the network object's status is only a fallback.
        online: online(module_status.as_deref()).or_else(|| online(status.as_deref())),
        has_sync_module: sync_module_id.is_some(),
        status,
        serial: owned_text(source, "serial")
            .or_else(|| module.and_then(|value| owned_text(value, "serial"))),
        firmware: owned_text(source, "fw_version")
            .or_else(|| module.and_then(|value| owned_text(value, "fw_version"))),
        sync_module_id,
    }
}
