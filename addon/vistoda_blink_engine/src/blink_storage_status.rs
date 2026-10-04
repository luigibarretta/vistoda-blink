use serde::Serialize;

use crate::{
    blink_api,
    blink_client::{BlinkClient, BlinkError},
    blink_storage::{LocalStorageStatus, storage_status},
};

/// Status-only view used for the native 30-second refresh. Unlike the
/// inventory it never asks the Sync Module to rebuild its USB manifest.
#[derive(Debug, Serialize)]
pub struct LocalStorageSummary {
    pub network_id: String,
    pub network_name: String,
    pub sync_module_id: String,
    pub sync_module_firmware: Option<String>,
    pub sync_module_status: Option<String>,
    pub status: LocalStorageStatus,
}

impl BlinkClient {
    pub async fn local_storage_statuses(&self) -> Result<Vec<LocalStorageSummary>, BlinkError> {
        let context = self.context().await?;
        let mut result = Vec::new();
        let mut failure = None;
        for network in self.state().await.networks {
            let Some(sync) = network.sync_module_id.clone() else {
                continue;
            };
            let path = blink_api::local_storage_status(&context.account_id, &network.id, &sync);
            // One Sync Module without USB support or a transient error must not
            // hide the others; authentication failures still propagate.
            let raw = match self.get_json(&context, &path).await {
                Ok(raw) => raw,
                Err(error @ (BlinkError::Authentication | BlinkError::OAuth(_))) => {
                    return Err(error);
                }
                Err(error) => {
                    tracing::warn!(%error, "Blink USB status unavailable for one Sync Module");
                    failure = Some(error);
                    continue;
                }
            };
            result.push(LocalStorageSummary {
                network_id: network.id,
                network_name: network.name,
                sync_module_id: sync,
                sync_module_firmware: network.firmware,
                sync_module_status: network.status,
                status: storage_status(&raw),
            });
        }
        match failure {
            Some(error) if result.is_empty() => Err(error),
            _ => Ok(result),
        }
    }
}
