use std::time::Duration;

use serde_json::Value;

use crate::{
    blink_api,
    blink_client::{BlinkClient, BlinkError, RequestContext},
    blink_model::CameraState,
    blink_settings,
    blink_zone_model::{
        CameraZones, CameraZonesUpdate, parse_zones, provider_body, same_values, update_body,
        validated_update,
    },
};

const VERIFY_ATTEMPTS: usize = 4;

impl BlinkClient {
    pub async fn camera_zones_state(&self, alias: &str) -> Result<CameraZones, BlinkError> {
        let (camera, _, raw, privacy_supported) = self.read_v1_zones(alias).await?;
        parse_zones(&camera.alias, &raw, privacy_supported)
    }

    pub async fn update_camera_zones(
        &self,
        alias: &str,
        input: &CameraZonesUpdate,
    ) -> Result<CameraZones, BlinkError> {
        if input.revision.len() != 64 {
            return Err(BlinkError::InvalidSetting);
        }
        let _guard = self.inner.settings_lock.lock().await;
        let (camera, context, raw, privacy_supported) = self.read_v1_zones(alias).await?;
        let before = parse_zones(&camera.alias, &raw, privacy_supported)?;
        if before.revision != input.revision {
            return Err(BlinkError::SettingsConflict);
        }
        let desired = validated_update(&before, input)?;
        if same_values(&before, &desired) {
            return Ok(before);
        }
        let original_body = provider_body(&raw)?;
        let desired_body = update_body(&original_body, &desired)?;
        self.write_zones(&context, &camera, desired_body).await?;
        if let Some(verified) = self.verify_zones(&context, &camera, &desired).await? {
            return Ok(verified);
        }
        self.write_zones(&context, &camera, original_body).await?;
        if self
            .verify_zones(&context, &camera, &before)
            .await?
            .is_none()
        {
            return Err(BlinkError::SettingsVerification);
        }
        Err(BlinkError::SettingsVerification)
    }

    async fn read_v1_zones(
        &self,
        alias: &str,
    ) -> Result<(CameraState, RequestContext, Value, bool), BlinkError> {
        let (camera, context, config_response) = self.read_settings(alias).await?;
        let config = blink_settings::config_object(&config_response);
        if camera.camera_type != "default"
            || config.get("zone_version").and_then(Value::as_str) != Some("v1")
            || config
                .get("motion_regions_compatible")
                .and_then(Value::as_bool)
                == Some(false)
        {
            return Err(BlinkError::SettingsUnsupported);
        }
        let privacy_supported = config
            .get("privacy_zones_compatible")
            .and_then(Value::as_bool)
            == Some(true);
        let raw = self
            .get_json(
                &context,
                &blink_api::camera_legacy_zones(&camera, &context.account_id),
            )
            .await?;
        Ok((camera, context, raw, privacy_supported))
    }

    async fn write_zones(
        &self,
        context: &RequestContext,
        camera: &CameraState,
        body: Value,
    ) -> Result<(), BlinkError> {
        let response = self
            .post_json(
                context,
                &blink_api::camera_legacy_zones(camera, &context.account_id),
                Some(body),
            )
            .await?;
        self.wait_command(context, response).await
    }

    async fn verify_zones(
        &self,
        context: &RequestContext,
        camera: &CameraState,
        expected: &CameraZones,
    ) -> Result<Option<CameraZones>, BlinkError> {
        for attempt in 0..VERIFY_ATTEMPTS {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(750)).await;
            }
            let raw = self
                .get_json(
                    context,
                    &blink_api::camera_legacy_zones(camera, &context.account_id),
                )
                .await?;
            let actual = parse_zones(&camera.alias, &raw, expected.privacy_supported)?;
            if same_values(&actual, expected) {
                return Ok(Some(actual));
            }
        }
        Ok(None)
    }
}
