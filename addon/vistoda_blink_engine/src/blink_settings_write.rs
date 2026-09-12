use std::time::Duration;

use crate::blink_setting_helpers::encode_vendor_value;
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::{
    blink_api,
    blink_client::{BlinkClient, BlinkError, RequestContext},
    blink_model::CameraState,
    blink_settings::{self, CameraSettings, SettingField, SettingKind},
};

const VERIFY_ATTEMPTS: usize = 4;

#[derive(Debug, Deserialize)]
pub struct CameraSettingUpdate {
    pub key: String,
    pub value: Value,
    pub revision: String,
}

impl BlinkClient {
    pub async fn camera_settings(&self, alias: &str) -> Result<CameraSettings, BlinkError> {
        let camera = self.camera(alias).await?;
        if !matches!(camera.camera_type.as_str(), "default" | "mini") {
            return Ok(blink_settings::parse(&camera, &Value::Null));
        }
        let (camera, _, response) = self.read_settings(alias).await?;
        Ok(blink_settings::parse(&camera, &response))
    }

    pub async fn update_camera_setting(
        &self,
        alias: &str,
        input: &CameraSettingUpdate,
    ) -> Result<CameraSettings, BlinkError> {
        if input.key.len() > 64 || input.revision.len() != 64 {
            return Err(BlinkError::InvalidSetting);
        }
        let _guard = self.inner.settings_lock.lock().await;
        let (camera, context, response) = self.read_settings(alias).await?;
        let before = blink_settings::parse(&camera, &response);
        if before.revision != input.revision {
            return Err(BlinkError::SettingsConflict);
        }
        if input.key == "temperature_thresholds" {
            return self
                .initialize_temperature_thresholds(&context, &camera, &before, &input.value)
                .await;
        }
        let field = before
            .settings
            .iter()
            .find(|item| item.key == input.key)
            .ok_or(BlinkError::SettingsUnsupported)?;
        let desired = validated_value(field, &input.value)?;
        if desired == field.value {
            return Ok(before);
        }
        let vendor_key = vendor_key(&camera, &input.key).ok_or(BlinkError::SettingsUnsupported)?;
        let current_vendor = blink_settings::config_object(&response)
            .get(vendor_key)
            .cloned()
            .ok_or(BlinkError::SettingsUnsupported)?;
        let desired_vendor = encode_vendor_value(&input.key, &current_vendor, &desired)?;
        self.write_setting(&context, &camera, &input.key, vendor_key, desired_vendor)
            .await?;
        if let Some(settings) = self
            .verify_setting(&context, &camera, &input.key, &desired)
            .await?
        {
            self.refresh_state().await?;
            return Ok(settings);
        }
        self.write_setting(&context, &camera, &input.key, vendor_key, current_vendor)
            .await?;
        let restored = self
            .verify_setting(&context, &camera, &input.key, &field.value)
            .await?;
        if restored.is_none() {
            return Err(BlinkError::SettingsVerification);
        }
        Err(BlinkError::SettingsVerification)
    }

    pub(crate) async fn read_settings(
        &self,
        alias: &str,
    ) -> Result<(CameraState, RequestContext, Value), BlinkError> {
        let camera = self.camera(alias).await?;
        if !matches!(camera.camera_type.as_str(), "default" | "mini") {
            return Err(BlinkError::SettingsUnsupported);
        }
        let context = self.context().await?;
        let response = self
            .get_json(
                &context,
                &blink_api::camera_config(&camera, &context.account_id),
            )
            .await?;
        Ok((camera, context, response))
    }

    async fn write_setting(
        &self,
        context: &RequestContext,
        camera: &CameraState,
        setting_key: &str,
        vendor_key: &str,
        value: Value,
    ) -> Result<(), BlinkError> {
        if matches!(setting_key, "temperature_min" | "temperature_max") {
            return self
                .write_temperature_threshold(context, camera, setting_key, value)
                .await;
        }
        if setting_key == "temperature_alerts" {
            let enabled = crate::blink_setting_helpers::bool_value(Some(&value))
                .ok_or(BlinkError::InvalidSetting)?;
            return self
                .post_ack(
                    context,
                    &blink_api::temperature_alert(camera, &context.account_id, enabled),
                )
                .await;
        }
        let body = Value::Object(Map::from_iter([(vendor_key.to_owned(), value)]));
        let response = self
            .post_json(
                context,
                &blink_api::camera_update(camera, &context.account_id),
                Some(body),
            )
            .await?;
        self.wait_command(context, response).await
    }

    async fn verify_setting(
        &self,
        context: &RequestContext,
        camera: &CameraState,
        key: &str,
        expected: &Value,
    ) -> Result<Option<CameraSettings>, BlinkError> {
        for attempt in 0..VERIFY_ATTEMPTS {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(750)).await;
            }
            let response = self
                .get_json(
                    context,
                    &blink_api::camera_config(camera, &context.account_id),
                )
                .await?;
            let settings = blink_settings::parse(camera, &response);
            if settings
                .settings
                .iter()
                .any(|item| item.key == key && &item.value == expected)
            {
                return Ok(Some(settings));
            }
        }
        Ok(None)
    }
}

fn validated_value(field: &SettingField, value: &Value) -> Result<Value, BlinkError> {
    if !field.writable {
        return Err(BlinkError::SettingsUnsupported);
    }
    match field.kind {
        SettingKind::Boolean if value.is_boolean() => Ok(value.clone()),
        SettingKind::Integer => {
            let number = value.as_i64().ok_or(BlinkError::InvalidSetting)?;
            let min = field.min.ok_or(BlinkError::InvalidSetting)?;
            let max = field.max.ok_or(BlinkError::InvalidSetting)?;
            let step = field.step.ok_or(BlinkError::InvalidSetting)?;
            if number < min || number > max || (number - min) % step != 0 {
                Err(BlinkError::InvalidSetting)
            } else {
                Ok(Value::from(number))
            }
        }
        SettingKind::Select => {
            let choice = value.as_str().ok_or(BlinkError::InvalidSetting)?;
            field
                .options
                .iter()
                .any(|item| item == choice)
                .then(|| Value::from(choice))
                .ok_or(BlinkError::InvalidSetting)
        }
        SettingKind::Text => {
            let text = value.as_str().ok_or(BlinkError::InvalidSetting)?.trim();
            let max = usize::try_from(field.max.ok_or(BlinkError::InvalidSetting)?)
                .map_err(|_| BlinkError::InvalidSetting)?;
            if text.is_empty() || text.chars().count() > max || text.chars().any(char::is_control) {
                Err(BlinkError::InvalidSetting)
            } else {
                Ok(Value::from(text))
            }
        }
        SettingKind::Boolean => Err(BlinkError::InvalidSetting),
    }
}

fn vendor_key(camera: &CameraState, key: &str) -> Option<&'static str> {
    Some(match key {
        "motion_detection" => "enabled",
        "video_recording" => "video_recording_enable",
        "audio_streaming" => "record_audio_enable",
        "clip_length" if camera.camera_type == "mini" => "clip_length",
        "clip_length" => "video_length",
        "video_quality" => "video_quality",
        "end_clip_early" => "early_termination",
        "night_vision" => "illuminator_enable",
        "ir_intensity" => "illuminator_intensity",
        "motion_sensitivity" => "motion_sensitivity",
        "retrigger_time" if camera.camera_type == "mini" => "retrigger_time",
        "retrigger_time" => "alert_interval",
        "early_notification" => "early_notification",
        "flip_video" => "flip_video",
        "photo_capture" => "snapshot_enabled",
        "auto_thumbnail" => "auto_update_thumbnail_enabled",
        "status_led" => "led_state",
        "speaker_volume" if camera.camera_type == "mini" => "volume_control",
        "camera_name" => "name",
        "temperature_alerts" => "temp_alarm_enable",
        "temperature_min" => "temp_min",
        "temperature_max" => "temp_max",
        _ => return None,
    })
}
