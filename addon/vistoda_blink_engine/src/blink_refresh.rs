use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::Value;
use time::OffsetDateTime;

use crate::{
    blink_api,
    blink_client::{BlinkClient, BlinkError},
    blink_model::{self, ProviderState},
};

impl BlinkClient {
    pub async fn refresh_state(&self) -> Result<(), BlinkError> {
        let context = self.context().await?;
        let home = self
            .get_json(&context, &blink_api::homescreen(&context.account_id))
            .await?;
        let network_catalog = self.get_json(&context, blink_api::networks()).await?;
        let usage = self.get_json(&context, "/api/v1/camera/usage").await?;
        let clips = self.recent_clips(&context).await;
        let network_updates = self.network_updates(&context, &network_catalog).await;
        let shell = blink_model::cameras(
            &context.account_id,
            &context.base_url,
            &usage,
            &home,
            &HashMap::new(),
            &HashMap::new(),
            &clips,
        );
        let (details, signals) = self.camera_details(&context, &shell).await;
        let cameras = blink_model::cameras(
            &context.account_id,
            &context.base_url,
            &usage,
            &home,
            &details,
            &signals,
            &clips,
        );
        *self.inner.state.write().await = ProviderState {
            account_id: context.account_id,
            updated_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            networks: blink_model::networks(&network_catalog, &home, &network_updates),
            cameras,
            clips,
        };
        Ok(())
    }

    async fn recent_clips(
        &self,
        context: &crate::blink_client::RequestContext,
    ) -> Vec<blink_model::MediaClip> {
        let since = one_hour_ago();
        let value = self
            .get_json(context, &blink_api::media(&context.account_id, &since, 1))
            .await
            .unwrap_or(Value::Null);
        blink_model::media(&value)
    }

    async fn network_updates(
        &self,
        context: &crate::blink_client::RequestContext,
        catalog: &Value,
    ) -> HashMap<String, Value> {
        let mut result = HashMap::new();
        let summaries = catalog.get("summary").and_then(Value::as_object);
        for (key, item) in summaries.into_iter().flatten() {
            let id = value_id(item, "id").unwrap_or_else(|| key.clone());
            if item.get("onboarded").and_then(Value::as_bool) == Some(false) {
                continue;
            }
            if let Ok(mut value) = self
                .post_json(context, &blink_api::network_update(&id), None)
                .await
            {
                let _ = self.wait_command(context, value.clone()).await;
                if let Ok(sync) = self
                    .get_json(context, &format!("/network/{id}/syncmodules"))
                    .await
                    && let Some(object) = value.as_object_mut()
                {
                    object.insert("_vistoda_sync".to_owned(), sync);
                }
                result.insert(id, value);
            }
        }
        result
    }

    async fn camera_details(
        &self,
        context: &crate::blink_client::RequestContext,
        cameras: &[blink_model::CameraState],
    ) -> (HashMap<String, Value>, HashMap<String, Value>) {
        let mut details = HashMap::new();
        let mut signals = HashMap::new();
        for camera in cameras
            .iter()
            .filter(|camera| camera.camera_type == "default")
        {
            if let Ok(value) = self
                .get_json(
                    context,
                    &blink_api::camera_config(camera, &context.account_id),
                )
                .await
            {
                let detail = value
                    .get("camera")
                    .and_then(Value::as_array)
                    .and_then(|values| values.first())
                    .cloned()
                    .unwrap_or(value);
                details.insert(camera.id.clone(), detail);
            }
            let path = format!(
                "/network/{}/camera/{}/signals",
                camera.network_id, camera.id
            );
            if let Ok(value) = self.get_json(context, &path).await {
                signals.insert(camera.id.clone(), value);
            }
        }
        (details, signals)
    }
}

fn one_hour_ago() -> String {
    blink_timestamp(OffsetDateTime::now_utc() - time::Duration::hours(1))
}

fn blink_timestamp(value: OffsetDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}+0000",
        value.year(),
        u8::from(value.month()),
        value.day(),
        value.hour(),
        value.minute(),
        value.second()
    )
}

fn value_id(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(|item| {
        item.as_str()
            .map(ToOwned::to_owned)
            .or_else(|| item.as_u64().map(|number| number.to_string()))
    })
}

#[cfg(test)]
mod tests {
    use super::blink_timestamp;
    use time::OffsetDateTime;

    #[test]
    fn uses_the_vendor_media_timestamp_contract() {
        let Ok(epoch) = OffsetDateTime::from_unix_timestamp(0) else {
            panic!("Unix epoch must be representable");
        };
        assert_eq!(blink_timestamp(epoch), "1970-01-01T00:00:00+0000");
    }
}
