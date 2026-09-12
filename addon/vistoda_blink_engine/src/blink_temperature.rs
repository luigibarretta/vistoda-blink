//! Blink's temperature thresholds are Fahrenheit integers, not general config writes.
use serde_json::{Value, json};

use crate::{
    blink_api,
    blink_client::{BlinkClient, BlinkError, RequestContext},
    blink_model::CameraState,
    blink_setting_helpers::{add_bool, add_integer, integer_value},
    blink_settings::{CameraSettings, SettingField, SettingKind, config_object},
};

pub fn fields(fields: &mut Vec<SettingField>, response: &Value, camera: &CameraState) {
    let source = config_object(response);
    let supported = camera.camera_type == "default";
    add_bool(
        fields,
        source,
        "temperature_alerts",
        "temp_alarm_enable",
        supported,
    );
    // Missing thresholds stay explicitly null, never presented as saved defaults.
    let writable = supported
        && response
            .pointer("/signals/temp")
            .and_then(Value::as_i64)
            .is_some();
    let limits = if camera.product_type == "catalina_indoor" {
        (32, 95, 1)
    } else {
        (-4, 113, 1)
    };
    add_integer(
        fields,
        source,
        "temperature_min",
        "temp_min",
        limits,
        writable,
    );
    if supported && source.get("temp_alarm_enable").is_some() {
        for (key, vendor) in [
            ("temperature_min", "temp_min"),
            ("temperature_max", "temp_max"),
        ] {
            if integer_value(source.get(vendor)).is_none() {
                fields.push(SettingField {
                    key: key.into(),
                    value: Value::Null,
                    kind: SettingKind::Integer,
                    writable,
                    min: Some(limits.0),
                    max: Some(limits.1),
                    step: Some(1),
                    options: Vec::new(),
                });
            }
        }
    }
    add_integer(
        fields,
        source,
        "temperature_max",
        "temp_max",
        limits,
        writable,
    );
}

fn threshold_body(response: &Value, key: &str, value: &Value) -> Result<Value, BlinkError> {
    let source = config_object(response);
    let mut low = integer_value(source.get("temp_min")).ok_or(BlinkError::SettingsUnsupported)?;
    let mut high = integer_value(source.get("temp_max")).ok_or(BlinkError::SettingsUnsupported)?;
    let current = response
        .pointer("/signals/temp")
        .and_then(Value::as_i64)
        .ok_or(BlinkError::SettingsUnsupported)?;
    let desired = value.as_i64().ok_or(BlinkError::InvalidSetting)?;
    match key {
        "temperature_min" => low = desired,
        "temperature_max" => high = desired,
        _ => return Err(BlinkError::InvalidSetting),
    }
    if high.checked_sub(low).is_none_or(|gap| gap < 10) {
        return Err(BlinkError::InvalidSetting);
    }
    // The API requires all three values. Echo the fresh measured value: do not
    // change the calibration when the user only changes an alert threshold.
    Ok(json!({"temp_min": low, "temp_max": high, "current_temp": current}))
}

impl BlinkClient {
    pub(crate) async fn initialize_temperature_thresholds(
        &self,
        context: &RequestContext,
        camera: &CameraState,
        before: &CameraSettings,
        value: &Value,
    ) -> Result<CameraSettings, BlinkError> {
        let object = value.as_object().ok_or(BlinkError::InvalidSetting)?;
        if object.len() != 2 || camera.camera_type != "default" {
            return Err(BlinkError::InvalidSetting);
        }
        let mut missing = false;
        for key in ["temperature_min", "temperature_max"] {
            let field = before
                .settings
                .iter()
                .find(|field| field.key == key)
                .ok_or(BlinkError::SettingsUnsupported)?;
            missing |= field.value.is_null();
            let desired = object
                .get(key)
                .and_then(Value::as_i64)
                .ok_or(BlinkError::InvalidSetting)?;
            if !field.writable
                || desired < field.min.unwrap_or(i64::MAX)
                || desired > field.max.unwrap_or(i64::MIN)
            {
                return Err(BlinkError::InvalidSetting);
            }
        }
        if !missing {
            return Err(BlinkError::SettingsConflict);
        }
        let response = self
            .get_json(
                context,
                &blink_api::camera_config(camera, &context.account_id),
            )
            .await?;
        if crate::blink_settings::parse(camera, &response).revision != before.revision {
            return Err(BlinkError::SettingsConflict);
        }
        let current = response
            .pointer("/signals/temp")
            .and_then(Value::as_i64)
            .ok_or(BlinkError::SettingsUnsupported)?;
        let low = &object["temperature_min"];
        let high = &object["temperature_max"];
        // Reuse the same gap/type checks without inventing a stored counterpart.
        let body = threshold_body(
            &json!({"temp_min":low,"temp_max":high,
            "signals":{"temp":current}}),
            "temperature_max",
            high,
        )?;
        self.post_temperature_body(context, camera, body).await?;
        for attempt in 0..4 {
            if attempt > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(750)).await;
            }
            let response = self
                .get_json(
                    context,
                    &blink_api::camera_config(camera, &context.account_id),
                )
                .await?;
            let after = crate::blink_settings::parse(camera, &response);
            if object.iter().all(|(key, value)| {
                after
                    .settings
                    .iter()
                    .any(|field| &field.key == key && &field.value == value)
            }) {
                return Ok(after);
            }
        }
        // Blink has no documented operation to restore an absent threshold.
        // Do not silently reset values or claim that initialization was rolled back.
        Err(BlinkError::SettingsVerification)
    }

    pub(crate) async fn write_temperature_threshold(
        &self,
        context: &RequestContext,
        camera: &CameraState,
        key: &str,
        value: Value,
    ) -> Result<(), BlinkError> {
        if camera.camera_type != "default" {
            return Err(BlinkError::SettingsUnsupported);
        }
        let response = self
            .get_json(
                context,
                &blink_api::camera_config(camera, &context.account_id),
            )
            .await?;
        let body = threshold_body(&response, key, &value)?;
        self.post_temperature_body(context, camera, body).await
    }

    async fn post_temperature_body(
        &self,
        context: &RequestContext,
        camera: &CameraState,
        body: Value,
    ) -> Result<(), BlinkError> {
        let path = format!(
            "/api/v1/accounts/{}/networks/{}/cameras/{}/calibrate",
            context.account_id, camera.network_id, camera.id
        );
        let command = self.post_json(context, &path, Some(body)).await?;
        self.wait_command(context, command).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn threshold_change_preserves_other_threshold_and_calibration() -> Result<(), BlinkError> {
        let response = json!({"camera":[{"temp_min":32,"temp_max":95}],"signals":{"temp":84}});
        assert_eq!(
            threshold_body(&response, "temperature_max", &json!(100))?,
            json!({"temp_min":32,"temp_max":100,"current_temp":84})
        );
        assert_eq!(
            threshold_body(&response, "temperature_min", &json!(40))?,
            json!({"temp_min":40,"temp_max":95,"current_temp":84})
        );
        assert!(threshold_body(&response, "temperature_min", &json!(90)).is_err());
        assert!(threshold_body(&response, "temperature_max", &json!(35)).is_err());
        assert!(threshold_body(&response, "camera_name", &json!(80)).is_err());
        Ok(())
    }
    #[test]
    fn missing_live_measurement_must_not_recalibrate() {
        let response = json!({"camera":[{"temp_min":32,"temp_max":95}]});
        assert!(threshold_body(&response, "temperature_max", &json!(100)).is_err());
    }
}
