use serde_json::Value;

use crate::{
    blink_api,
    blink_client::{BlinkClient, BlinkError, RequestContext},
};

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
        if !status
            .get("usb_format_compatible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Err(BlinkError::InvalidStorageOperation);
        }
        let path = blink_api::local_storage_format(&context.account_id, &network, &sync);
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
