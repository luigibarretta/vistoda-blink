use std::time::Duration;

use bytes::Bytes;
use serde_json::Value;
use tracing::warn;

use crate::{
    blink_api::{self, CameraAction, LiveDescriptor},
    blink_client::{BlinkClient, BlinkError, RequestContext},
    blink_http::absolute,
    blink_model::CameraState,
};

const SNAPSHOT_LIMIT: usize = 16 * 1024 * 1024;
const VIDEO_LIMIT: usize = 128 * 1024 * 1024;

impl BlinkClient {
    pub async fn set_armed(&self, network_id: &str, armed: bool) -> Result<(), BlinkError> {
        if !self
            .state()
            .await
            .networks
            .iter()
            .any(|item| item.id == network_id)
        {
            return Err(BlinkError::NetworkNotFound);
        }
        let context = self.context().await?;
        let value = self
            .post_json(
                &context,
                &blink_api::arm(&context.account_id, network_id, armed),
                None,
            )
            .await?;
        self.wait_command(&context, value).await
    }

    pub async fn camera_command(
        &self,
        alias: &str,
        action: CameraAction,
    ) -> Result<(), BlinkError> {
        let camera = self.camera(alias).await?;
        let context = self.context().await?;
        let spec = blink_api::camera_action(&camera, &context.account_id, &action);
        let value = self.post_json(&context, &spec.path, spec.body).await?;
        self.wait_command(&context, value).await
    }

    pub async fn start_live(
        &self,
        alias: &str,
    ) -> Result<(CameraState, LiveDescriptor), BlinkError> {
        let camera = self.camera(alias).await?;
        let context = self.context().await?;
        let spec = blink_api::camera_action(&camera, &context.account_id, &CameraAction::Live);
        let value = self.post_json(&context, &spec.path, spec.body).await?;
        let descriptor = serde_json::from_value(value).map_err(|_| BlinkError::InvalidResponse)?;
        Ok((camera, descriptor))
    }

    pub async fn finish_live(&self, camera: &CameraState, command_id: u64) {
        if let Ok(context) = self.context().await {
            let _ = self
                .post_json(
                    &context,
                    &blink_api::live_command_done(
                        &context.account_id,
                        &camera.network_id,
                        command_id,
                    ),
                    None,
                )
                .await;
        }
    }

    pub async fn live_active(
        &self,
        camera: &CameraState,
        command_id: u64,
    ) -> Result<bool, BlinkError> {
        let context = self.context().await?;
        let status = self
            .get_json(
                &context,
                &blink_api::live_command(&context.account_id, &camera.network_id, command_id),
            )
            .await?;
        Ok(live_command_is_active(&status, command_id))
    }

    pub async fn snapshot(&self, alias: &str) -> Result<Bytes, BlinkError> {
        let camera = self.camera(alias).await?;
        let url = camera.thumbnail_url.ok_or(BlinkError::InvalidResponse)?;
        self.download(&url, SNAPSHOT_LIMIT).await
    }

    pub async fn clip(&self, id: &str) -> Result<Bytes, BlinkError> {
        let state = self.state().await;
        let clip = state
            .clips
            .iter()
            .find(|item| item.id == id)
            .ok_or(BlinkError::CameraNotFound)?;
        let context = self.context().await?;
        self.download(&absolute(&context.base_url, &clip.media_url), VIDEO_LIMIT)
            .await
    }

    async fn camera(&self, alias: &str) -> Result<CameraState, BlinkError> {
        self.state()
            .await
            .cameras
            .into_iter()
            .find(|item| item.alias == alias)
            .ok_or(BlinkError::CameraNotFound)
    }

    pub(crate) async fn wait_command(
        &self,
        context: &RequestContext,
        value: Value,
    ) -> Result<(), BlinkError> {
        let Some(id) = value.get("id").and_then(Value::as_u64) else {
            return Ok(());
        };
        let network = value
            .get("network_id")
            .and_then(Value::as_u64)
            .map(|item| item.to_string());
        let Some(network) = network else {
            return Ok(());
        };
        for _ in 0..120 {
            let status = self
                .get_json(context, &blink_api::command(&network, id))
                .await?;
            if !command_is_pending(&status) {
                return Ok(());
            }
            if status.get("complete").and_then(Value::as_bool) == Some(true) {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        warn!("Blink command polling timed out after provider acceptance");
        Ok(())
    }
}

fn live_command_is_active(status: &Value, command_id: u64) -> bool {
    if status.get("complete").and_then(Value::as_bool) == Some(true)
        || matches!(
            status.get("status").and_then(Value::as_str),
            Some("complete" | "failed")
        )
    {
        return false;
    }
    let legacy_running = status
        .get("commands")
        .and_then(Value::as_array)
        .and_then(|commands| {
            commands
                .iter()
                .find(|command| command.get("id").and_then(Value::as_u64) == Some(command_id))
        })
        .and_then(|command| command.get("state_condition"))
        .and_then(Value::as_str)
        .is_some_and(|state| matches!(state, "new" | "running"));
    matches!(
        status.get("status").and_then(Value::as_str),
        Some("new" | "running")
    ) || legacy_running
}

fn command_is_pending(status: &Value) -> bool {
    status.get("status_code").and_then(Value::as_u64) == Some(908)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{command_is_pending, live_command_is_active};

    #[test]
    fn treats_vendor_908_as_the_only_pending_status() {
        assert!(command_is_pending(&json!({"status_code": 908})));
        assert!(!command_is_pending(&json!({"status_code": 901})));
        assert!(!command_is_pending(&json!({})));
    }

    #[test]
    fn understands_current_and_legacy_live_command_states() {
        assert!(live_command_is_active(&json!({"status": "running"}), 7));
        assert!(live_command_is_active(
            &json!({"commands": [{"id": 7, "state_condition": "new"}]}),
            7
        ));
        assert!(!live_command_is_active(
            &json!({"status": "complete", "complete": true}),
            7
        ));
        assert!(!live_command_is_active(&json!({"status": "failed"}), 7));
    }
}
