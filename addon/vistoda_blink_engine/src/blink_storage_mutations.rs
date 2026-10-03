use serde_json::Value;

use crate::{
    blink_api,
    blink_client::{BlinkClient, BlinkError, RequestContext},
};

/// Native Sync Module storage commands (Blink Android 59.1 `SyncModuleApi`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageCommand {
    Format,
    Eject,
    Mount,
}

impl StorageCommand {
    const fn path(self) -> &'static str {
        match self {
            Self::Format => "format",
            Self::Eject => "eject",
            Self::Mount => "mount",
        }
    }

    /// Fresh provider status must still allow the command when it is sent.
    fn allowed(self, status: &Value) -> bool {
        let state = status.get("usb_state").and_then(Value::as_str);
        match self {
            Self::Format => status
                .get("usb_format_compatible")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            Self::Eject => matches!(state, Some("active" | "memory_full")),
            Self::Mount => state == Some("unmounted"),
        }
    }
}

impl BlinkClient {
    pub async fn delete_local_storage_clip(
        &self,
        network: u64,
        sync: u64,
        manifest: u64,
        clip: u64,
    ) -> Result<(), BlinkError> {
        let _guard = self.inner.storage_lock.lock().await;
        let context = self.context().await?;
        let network = network.to_string();
        let sync = sync.to_string();
        self.verify_storage_target(&context, &network, &sync, Some((manifest, clip)))
            .await?;
        let path = blink_api::local_storage_clip_delete(
            &context.account_id,
            &network,
            &sync,
            manifest,
            clip,
        );
        let requested = self.post_json(&context, &path, None).await?;
        self.wait_current_command(&context, &network, &requested)
            .await?;
        Ok(())
    }

    pub async fn format_local_storage(&self, network: u64, sync: u64) -> Result<(), BlinkError> {
        self.local_storage_command(network, sync, StorageCommand::Format)
            .await
    }

    pub async fn local_storage_command(
        &self,
        network: u64,
        sync: u64,
        command: StorageCommand,
    ) -> Result<(), BlinkError> {
        let _guard = self.inner.storage_lock.lock().await;
        let context = self.context().await?;
        let network = network.to_string();
        let sync = sync.to_string();
        self.verify_storage_target(&context, &network, &sync, None)
            .await?;
        let status = self
            .get_json(
                &context,
                &blink_api::local_storage_status(&context.account_id, &network, &sync),
            )
            .await?;
        if !command.allowed(&status) {
            return Err(BlinkError::InvalidStorageOperation);
        }
        let path =
            blink_api::local_storage_action(&context.account_id, &network, &sync, command.path());
        let requested = self.post_json(&context, &path, None).await?;
        self.wait_current_command(&context, &network, &requested)
            .await?;
        Ok(())
    }

    async fn verify_storage_target(
        &self,
        context: &RequestContext,
        network: &str,
        sync: &str,
        clip: Option<(u64, u64)>,
    ) -> Result<(), BlinkError> {
        let known = self
            .state()
            .await
            .networks
            .into_iter()
            .any(|item| item.id == network && item.sync_module_id.as_deref() == Some(sync));
        if !known {
            return Err(BlinkError::NetworkNotFound);
        }
        if let Some((manifest, clip)) = clip {
            let (current_manifest, clips) = self
                .load_local_storage_manifest(context, network, sync)
                .await?;
            if current_manifest != Some(manifest) || !clips.iter().any(|item| item.id == clip) {
                return Err(BlinkError::InvalidStorageOperation);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::StorageCommand;

    #[test]
    fn commands_follow_native_state_gates() {
        let active = json!({"usb_state": "active", "usb_format_compatible": true});
        let full = json!({"usb_state": "memory_full"});
        let unmounted = json!({"usb_state": "unmounted"});
        let damaged = json!({"usb_state": "format_required", "usb_format_compatible": true});
        assert!(StorageCommand::Eject.allowed(&active) && StorageCommand::Eject.allowed(&full));
        assert!(!StorageCommand::Eject.allowed(&unmounted));
        assert!(
            StorageCommand::Mount.allowed(&unmounted) && !StorageCommand::Mount.allowed(&active)
        );
        assert!(StorageCommand::Format.allowed(&damaged));
        assert!(!StorageCommand::Format.allowed(&json!({"usb_state": "format_required"})));
        assert_eq!(StorageCommand::Eject.path(), "eject");
        assert_eq!(StorageCommand::Mount.path(), "mount");
    }
}
