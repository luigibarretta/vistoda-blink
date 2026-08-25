use std::time::Duration;

use bytes::Bytes;
use reqwest::{Method, StatusCode};
use serde_json::Value;
use tracing::{debug, warn};

use crate::blink_client::{BlinkClient, BlinkError, RequestContext};

const MEDIA_TIMEOUT: Duration = Duration::from_secs(90);

impl BlinkClient {
    pub(crate) async fn get_json(
        &self,
        context: &RequestContext,
        path: &str,
    ) -> Result<Value, BlinkError> {
        self.json(Method::GET, context, path, None).await
    }

    pub(crate) async fn post_json(
        &self,
        context: &RequestContext,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value, BlinkError> {
        self.json(Method::POST, context, path, body).await
    }

    async fn json(
        &self,
        method: Method,
        context: &RequestContext,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value, BlinkError> {
        let response = self
            .send_json(method.clone(), context, path, body.as_ref())
            .await?;
        let response = if response.status() == StatusCode::UNAUTHORIZED {
            if let Some(session) = self.inner.session.lock().await.as_mut() {
                session.access = None;
            }
            let refreshed = self.context().await?;
            self.send_json(method, &refreshed, path, body.as_ref())
                .await?
        } else {
            response
        };
        if response.status() == StatusCode::UNAUTHORIZED {
            if let Some(session) = self.inner.session.lock().await.as_mut() {
                session.access = None;
            }
            warn!(
                endpoint = endpoint_class(path),
                "Blink cloud rejected refreshed access"
            );
            return Err(BlinkError::Authentication);
        }
        log_failure(response.status(), path);
        Ok(response.error_for_status()?.json().await?)
    }

    async fn send_json(
        &self,
        method: Method,
        context: &RequestContext,
        path: &str,
        body: Option<&Value>,
    ) -> Result<reqwest::Response, BlinkError> {
        let mut request = self
            .inner
            .http
            .request(method, absolute(&context.base_url, path))
            .bearer_auth(context.token.as_str());
        if let Some(body) = body {
            request = request.json(body);
        }
        Ok(request.send().await?)
    }

    pub(crate) async fn download(&self, url: &str, limit: usize) -> Result<Bytes, BlinkError> {
        let context = self.context().await?;
        let response = self
            .inner
            .http
            .get(url)
            .bearer_auth(context.token.as_str())
            .timeout(MEDIA_TIMEOUT)
            .send()
            .await?
            .error_for_status()?;
        if response
            .content_length()
            .is_some_and(|size| size > limit as u64)
        {
            return Err(BlinkError::MediaTooLarge);
        }
        let bytes = response.bytes().await?;
        if bytes.len() > limit {
            return Err(BlinkError::MediaTooLarge);
        }
        Ok(bytes)
    }
}

fn log_failure(status: StatusCode, path: &str) {
    if status.is_success() {
        return;
    }
    if status == StatusCode::NOT_FOUND && is_optional_sync_module(path) {
        debug!(
            endpoint = endpoint_class(path),
            status = status.as_u16(),
            "Blink account has no optional Sync Module endpoint"
        );
        return;
    }
    warn!(
        endpoint = endpoint_class(path),
        status = status.as_u16(),
        "Blink cloud request failed"
    );
}

fn endpoint_class(path: &str) -> &'static str {
    if path.contains("homescreen") {
        "homescreen"
    } else if path == "/networks" {
        "networks"
    } else if path.contains("camera/usage") {
        "camera_usage"
    } else if path.contains("media/changed") {
        "media"
    } else if path.contains("/command/") {
        "command"
    } else if path.contains("/config") {
        "camera_config"
    } else if path.contains("/signals") {
        "camera_signals"
    } else if is_optional_sync_module(path) {
        "sync_module"
    } else {
        "provider"
    }
}

fn is_optional_sync_module(path: &str) -> bool {
    path.starts_with("/network/") && path.ends_with("/syncmodules")
}

pub fn absolute(base: &str, path: &str) -> String {
    if path.starts_with("http") {
        path.to_owned()
    } else {
        format!("{base}{path}")
    }
}

#[cfg(test)]
mod tests {
    use super::{endpoint_class, is_optional_sync_module};

    #[test]
    fn classifies_optional_sync_module_without_provider_noise() {
        let path = "/network/123/syncmodules";
        assert!(is_optional_sync_module(path));
        assert_eq!(endpoint_class(path), "sync_module");
        assert!(!is_optional_sync_module("/network/123/camera/456/signals"));
    }
}
