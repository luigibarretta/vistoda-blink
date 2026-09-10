use bytes::Bytes;
use serde::Serialize;
use serde_json::Value;

use crate::{
    blink_api,
    blink_client::{BlinkClient, BlinkError},
    blink_http::absolute,
    error::EngineError,
    pagination::{self, Pagination},
};

const CLIP_LIMIT: usize = 128 * 1024 * 1024;
const INVENTORY_LIMIT: usize = 1_000;

#[derive(Debug, Serialize)]
pub struct LocalStorageInventory {
    pub network_id: String,
    pub network_name: String,
    pub sync_module_id: String,
    pub status: LocalStorageStatus,
    pub manifest_id: Option<u64>,
    pub clips: Vec<LocalStorageClip>,
    pub pagination: Pagination,
}

#[derive(Debug, Default, Serialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct LocalStorageStatus {
    pub enabled: bool,
    pub usb_state: String,
    pub usb_storage_used: Option<u64>,
    pub usb_storage_available_percentage: Option<u64>,
    pub usb_storage_full: bool,
    pub can_delete_clips: bool,
    pub can_format_usb: bool,
    pub backup_enabled: bool,
    pub backup_in_progress: bool,
    pub last_backup_completed: Option<String>,
    pub last_backup_result: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LocalStorageClip {
    pub id: u64,
    pub device_name: String,
    pub created_at: String,
    pub event_type: Option<String>,
    pub clip_length_ms: Option<u64>,
    pub media_available: bool,
}

impl BlinkClient {
    pub async fn local_storage_inventories(
        &self,
        page: Option<usize>,
        page_size: Option<usize>,
    ) -> Result<Vec<LocalStorageInventory>, EngineError> {
        let _guard = self.inner.storage_lock.lock().await;
        let context = self.context().await?;
        let networks = self.state().await.networks;
        let mut result = Vec::new();
        for network in networks {
            let Some(sync) = network.sync_module_id.clone() else {
                continue;
            };
            let raw_status = self
                .get_json(
                    &context,
                    &blink_api::local_storage_status(&context.account_id, &network.id, &sync),
                )
                .await?;
            let status = storage_status(&raw_status);
            // ACTIVE and MEMORY_FULL are the native app's authoritative
            // readable states; Blink's `enabled` compatibility flag can lag.
            let (manifest_id, mut clips) = if has_readable_media(&status.usb_state) {
                self.load_local_storage_manifest(&context, &network.id, &sync)
                    .await?
            } else {
                (None, Vec::new())
            };
            clips.sort_by(|left, right| right.created_at.cmp(&left.created_at));
            let (clips, pagination) = pagination::page(&clips, page, page_size)?;
            result.push(LocalStorageInventory {
                network_id: network.id,
                network_name: network.name,
                sync_module_id: sync,
                status,
                manifest_id,
                clips,
                pagination,
            });
        }
        Ok(result)
    }

    pub(crate) async fn load_local_storage_manifest(
        &self,
        context: &crate::blink_client::RequestContext,
        network: &str,
        sync: &str,
    ) -> Result<(Option<u64>, Vec<LocalStorageClip>), BlinkError> {
        let requested = self
            .post_json(
                context,
                &blink_api::local_storage_manifest_request(&context.account_id, network, sync),
                None,
            )
            .await?;
        let completed = self
            .wait_current_command(context, network, &requested)
            .await?;
        let command = child_command_id(&completed)
            .or_else(|| requested.get("id").and_then(Value::as_u64))
            .ok_or(BlinkError::InvalidResponse)?;
        let media = self
            .get_json(
                context,
                &blink_api::local_storage_media(&context.account_id, network, sync, command),
            )
            .await?;
        Ok(parse_manifest(&media))
    }

    pub async fn local_storage_clip(
        &self,
        network: u64,
        sync: u64,
        manifest: u64,
        clip: u64,
    ) -> Result<Bytes, BlinkError> {
        let _guard = self.inner.storage_lock.lock().await;
        let context = self.context().await?;
        let path = blink_api::local_storage_clip_request(
            &context.account_id,
            &network.to_string(),
            &sync.to_string(),
            manifest,
            clip,
        );
        let requested = self.post_json(&context, &path, None).await?;
        self.wait_current_command(&context, &network.to_string(), &requested)
            .await?;
        self.download(&absolute(&context.base_url, &path), CLIP_LIMIT)
            .await
    }
}

fn child_command_id(value: &Value) -> Option<u64> {
    value
        .get("commands")?
        .as_array()?
        .first()?
        .get("id")?
        .as_u64()
}

fn has_readable_media(state: &str) -> bool {
    matches!(state, "active" | "memory_full")
}

fn storage_status(value: &Value) -> LocalStorageStatus {
    let used = number(value, "usb_storage_used").filter(|value| *value <= 100);
    let readable = text(value, "usb_state").is_some_and(|state| has_readable_media(&state));
    LocalStorageStatus {
        enabled: boolean(value, "enabled"),
        usb_state: text(value, "usb_state").unwrap_or_default(),
        usb_storage_used: used,
        usb_storage_available_percentage: used.map(|value| 100 - value),
        usb_storage_full: boolean(value, "usb_storage_full"),
        can_delete_clips: readable,
        can_format_usb: boolean(value, "usb_format_compatible"),
        backup_enabled: boolean(value, "sm_backup_enabled"),
        backup_in_progress: boolean(value, "sm_backup_in_progress"),
        last_backup_completed: optional_text(value, "last_backup_completed"),
        last_backup_result: optional_text(value, "last_backup_result"),
    }
}

fn parse_manifest(value: &Value) -> (Option<u64>, Vec<LocalStorageClip>) {
    let manifest = number(value, "manifest_id");
    let clips = value
        .get("media")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .take(INVENTORY_LIMIT)
                .filter_map(parse_clip)
                .collect()
        })
        .unwrap_or_default();
    (manifest, clips)
}

fn parse_clip(value: &Value) -> Option<LocalStorageClip> {
    Some(LocalStorageClip {
        id: number(value, "id")?,
        device_name: text(value, "device_name").unwrap_or_else(|| "Telecamera Blink".into()),
        created_at: text(value, "created_at")
            .or_else(|| number(value, "clip_start_millis").map(|millis| millis.to_string()))
            .unwrap_or_default(),
        event_type: optional_text(value, "event_type"),
        clip_length_ms: number(value, "clip_length_ms")
            .or_else(|| number(value, "clip_length").map(|seconds| seconds * 1_000)),
        media_available: value.get("media").and_then(Value::as_str).is_some(),
    })
}

fn boolean(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}
fn number(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(Value::as_u64)
}
fn optional_text(value: &Value, key: &str) -> Option<String> {
    text(value, key)
}
fn text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

#[cfg(test)]
#[path = "blink_storage_tests.rs"]
mod tests;
