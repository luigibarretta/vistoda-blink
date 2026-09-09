use bytes::Bytes;
use serde::Serialize;
use serde_json::Value;

use crate::{
    blink_api,
    blink_client::{BlinkClient, BlinkError},
    blink_http::absolute,
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
}

#[derive(Debug, Default, Serialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct LocalStorageStatus {
    pub enabled: bool,
    pub usb_state: String,
    pub usb_storage_used: Option<u64>,
    pub usb_storage_full: bool,
    pub backup_enabled: bool,
    pub backup_in_progress: bool,
    pub last_backup_completed: Option<String>,
    pub last_backup_result: Option<String>,
}

#[derive(Debug, Serialize)]
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
    ) -> Result<Vec<LocalStorageInventory>, BlinkError> {
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
            let (manifest_id, clips) = if has_readable_media(&status.usb_state) {
                self.load_local_storage_manifest(&context, &network.id, &sync)
                    .await?
            } else {
                (None, Vec::new())
            };
            result.push(LocalStorageInventory {
                network_id: network.id,
                network_name: network.name,
                sync_module_id: sync,
                status,
                manifest_id,
                clips,
            });
        }
        Ok(result)
    }

    async fn load_local_storage_manifest(
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
    LocalStorageStatus {
        enabled: boolean(value, "enabled"),
        usb_state: text(value, "usb_state").unwrap_or_default(),
        usb_storage_used: number(value, "usb_storage_used"),
        usb_storage_full: boolean(value, "usb_storage_full"),
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
mod tests {
    use super::{child_command_id, has_readable_media, parse_manifest, storage_status};
    use serde_json::json;

    #[test]
    fn parses_only_read_only_status_and_bounded_clip_metadata() {
        let status = storage_status(&json!({"enabled": true, "usb_state": "mounted",
            "usb_storage_used": 31, "usb_storage_full": false, "can_format_usb": true}));
        assert!(status.enabled);
        assert_eq!(status.usb_state, "mounted");
        assert_eq!(status.usb_storage_used, Some(31));
        let (manifest, clips) = parse_manifest(&json!({"manifest_id": 8, "media": [{"id": 9,
            "device_name": "Balcone", "created_at": "2026-09-09T10:00:00Z",
            "clip_length_ms": 5000, "media": "/request/9"}, {"id": 10,
            "clip_start_millis": 1_788_948_000_000_u64}]}));
        assert_eq!(manifest, Some(8));
        assert_eq!(clips[0].id, 9);
        assert!(clips[0].media_available);
        assert_eq!(clips[1].created_at, "1788948000000");
    }

    #[test]
    fn extracts_manifest_child_command() {
        assert_eq!(
            child_command_id(&json!({"commands": [{"id": 73}]})),
            Some(73)
        );
    }

    #[test]
    fn requests_manifests_only_for_official_readable_usb_states() {
        assert!(has_readable_media("active"));
        assert!(has_readable_media("memory_full"));
        assert!(!has_readable_media("unmounted"));
        assert!(!has_readable_media("format_required"));
        assert!(!has_readable_media("unavailable"));
        assert!(!has_readable_media("incompatible"));
    }
}
