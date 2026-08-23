use std::time::Duration;

use bytes::Bytes;
use serde_json::Value;

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
                    &blink_api::command_done(&camera.network_id, command_id),
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
                &blink_api::command(&camera.network_id, command_id),
            )
            .await?;
        if status.get("status_code").and_then(Value::as_u64) != Some(908) {
            return Ok(false);
        }
        Ok(status
            .get("commands")
            .and_then(Value::as_array)
            .and_then(|commands| {
                commands
                    .iter()
                    .find(|command| command.get("id").and_then(Value::as_u64) == Some(command_id))
            })
            .and_then(|command| command.get("state_condition"))
            .and_then(Value::as_str)
            .is_none_or(|state| matches!(state, "new" | "running")))
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
            .map(|item| item.to_string())
            .ok_or(BlinkError::InvalidResponse)?;
        for _ in 0..120 {
            let status = self
                .get_json(context, &blink_api::command(&network, id))
                .await?;
            if status.get("status_code").and_then(Value::as_u64) != Some(908) {
                return Err(BlinkError::InvalidResponse);
            }
            if status.get("complete").and_then(Value::as_bool) == Some(true) {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        Err(BlinkError::CommandTimeout)
    }
}
