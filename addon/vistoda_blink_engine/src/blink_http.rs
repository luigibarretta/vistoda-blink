use std::time::Duration;

use bytes::Bytes;
use reqwest::{Method, StatusCode};
use serde_json::Value;

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
        if response.status() == StatusCode::UNAUTHORIZED {
            if let Some(session) = self.inner.session.lock().await.as_mut() {
                session.access = None;
            }
            let refreshed = self.context().await?;
            return Ok(self
                .send_json(method, &refreshed, path, body.as_ref())
                .await?
                .error_for_status()?
                .json()
                .await?);
        }
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

pub fn absolute(base: &str, path: &str) -> String {
    if path.starts_with("http") {
        path.to_owned()
    } else {
        format!("{base}{path}")
    }
}
