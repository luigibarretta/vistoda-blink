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
        for network in self.state().await.networks {
            let Some(sync) = network.sync_module_id.clone() else {
                continue;
            };
            let raw = self
                .get_json(
                    &context,
                    &blink_api::local_storage_status(&context.account_id, &network.id, &sync),
                )
                .await?;
            result.push(LocalStorageSummary {
                network_id: network.id,
                network_name: network.name,
                sync_module_id: sync,
                sync_module_firmware: network.firmware,
                sync_module_status: network.status,
                status: storage_status(&raw),
            });
        }
        Ok(result)
    }
}
