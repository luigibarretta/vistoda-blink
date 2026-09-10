use std::time::Duration;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::HeaderMap,
    routing::post,
};
use futures_util::SinkExt;
use reqwest::header::{AUTHORIZATION, HeaderValue, USER_AGENT};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Error as WebSocketError, Message, client::IntoClientRequest},
};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{
    api::{authorize, validate_alias},
    blink_client::{BlinkClient, BlinkError},
    error::EngineError,
    hub::EngineState,
};

const SIGNALING_URL: &str = "wss://api.prod.signalling.ring.devices.a2z.com/ws";
const PROTOCOL: &str = "4.1";
const AUTH_TYPE: &str = "blink_oauth";
const CLIENT_INFO: &str = "Blink/59.1; Platform/Android; OS/14; Density/3.0; Device/samsung-SM-G998B; Locale/en-US; TimeZone/UTC";

struct SignalingIdentity {
    token: Zeroizing<String>,
    hardware_id: String,
    user_id: String,
}

#[derive(Serialize)]
pub struct SignalingProbe {
    signaling_protocol: &'static str,
    signaling: &'static str,
    failure_phase: Option<&'static str>,
    http_status: Option<u16>,
    ring_device_identity: &'static str,
    device_support: &'static str,
    media_session: &'static str,
    full_duplex: &'static str,
}

pub fn routes() -> Router<EngineState> {
    Router::new().route("/v1/cameras/{alias}/audio/probe", post(probe))
}

async fn probe(
    State(state): State<EngineState>,
    headers: HeaderMap,
    Path(alias): Path<String>,
) -> Result<Json<SignalingProbe>, EngineError> {
    authorize(&state, &headers)?;
    validate_alias(&alias)?;
    Ok(Json(state.client().signaling_probe(&alias).await?))
}

impl BlinkClient {
    pub(crate) async fn signaling_target(
        &self,
        alias: &str,
    ) -> Result<(axum::http::Request<()>, u64), BlinkError> {
        let camera = self
            .state()
            .await
            .cameras
            .into_iter()
            .find(|camera| camera.alias == alias)
            .ok_or(BlinkError::CameraNotFound)?;
        let doorbot_id = camera.ring_device_id.ok_or(BlinkError::InvalidResponse)?;
        let identity = self.signaling_identity().await?;
        Ok((signaling_request(&identity)?, doorbot_id))
    }

    async fn signaling_identity(&self) -> Result<SignalingIdentity, BlinkError> {
        let context = self.context().await?;
        let (hardware_id, stored_user_id) = {
            let session = self.inner.session.lock().await;
            let credentials = &session.as_ref().ok_or(BlinkError::NotEnrolled)?.credentials;
            (credentials.hardware_id.clone(), credentials.user_id.clone())
        };
        let account = self.get_json(&context, "/api/v2/users/info").await?;
        // Signaling uses the shared Ring identity, not Blink's account/user id.
        // Imported official HA credentials can contain the latter, so never
        // trust that migrated field without reconciling it against the native
        // account endpoint first.
        let user_id = canonical_ring_user_id(&account).ok_or(BlinkError::InvalidResponse)?;
        if stored_user_id.as_deref() != Some(user_id.as_str()) {
            let credentials = {
                let mut session = self.inner.session.lock().await;
                let credentials = &mut session.as_mut().ok_or(BlinkError::NotEnrolled)?.credentials;
                credentials.user_id = Some(user_id.clone());
                credentials.clone()
            };
            self.inner.store.save(&credentials).await?;
        }
        Ok(SignalingIdentity {
            token: context.token,
            hardware_id,
            user_id,
        })
    }

    async fn signaling_probe(&self, alias: &str) -> Result<SignalingProbe, BlinkError> {
        let camera = self
            .state()
            .await
            .cameras
            .into_iter()
            .find(|camera| camera.alias == alias)
            .ok_or(BlinkError::CameraNotFound)?;
        let identity = self.signaling_identity().await?;
        let request = signaling_request(&identity)?;
        let (signaling, failure_phase, http_status) =
            match tokio::time::timeout(Duration::from_secs(12), connect_async(request)).await {
                Err(_) => ("timeout", Some("websocket_handshake"), None),
                Ok(Err(WebSocketError::Http(response))) => (
                    "rejected",
                    Some("websocket_handshake"),
                    Some(response.status().as_u16()),
                ),
                Ok(Err(_)) => ("transport_failed", Some("websocket_handshake"), None),
                Ok(Ok((mut socket, _))) => {
                    let _ = socket.send(Message::Close(None)).await;
                    ("authenticated", None, None)
                }
            };
        Ok(SignalingProbe {
            signaling_protocol: PROTOCOL,
            signaling,
            failure_phase,
            http_status,
            ring_device_identity: if camera.ring_device_id.is_some() {
                "present"
            } else {
                "missing"
            },
            device_support: match camera.two_way_audio {
                Some(true) => "advertised",
                Some(false) => "not_advertised",
                None => "unknown",
            },
            media_session: "not_started",
            full_duplex: if signaling == "authenticated" {
                "gated"
            } else {
                "blocked"
            },
        })
    }
}

fn text_or_number(value: &serde_json::Value, key: &str) -> Option<String> {
    value.get(key).and_then(|item| {
        item.as_str()
            .map(ToOwned::to_owned)
            .or_else(|| item.as_u64().map(|number| number.to_string()))
    })
}

fn canonical_ring_user_id(value: &serde_json::Value) -> Option<String> {
    text_or_number(value, "ringUserId")
        .or_else(|| text_or_number(value, "ring_user_id"))
        .filter(|value| value != "0")
}

fn signaling_request(identity: &SignalingIdentity) -> Result<axum::http::Request<()>, BlinkError> {
    let mut request = SIGNALING_URL
        .into_client_request()
        .map_err(|_| BlinkError::InvalidResponse)?;
    let headers = request.headers_mut();
    insert(headers, "x-sig-api-version", PROTOCOL)?;
    insert(headers, "x-sig-client-id", &client_id(identity))?;
    insert(headers, "x-sig-auth-type", AUTH_TYPE)?;
    insert(headers, "x-app-correlation-id", &Uuid::new_v4().to_string())?;
    headers.insert(USER_AGENT, header(CLIENT_INFO)?);
    headers.insert(
        AUTHORIZATION,
        header(&format!("Bearer {}", identity.token.as_str()))?,
    );
    Ok(request)
}

fn client_id(identity: &SignalingIdentity) -> String {
    let mut digest = Sha256::new();
    digest.update(identity.hardware_id.as_bytes());
    digest.update(identity.user_id.as_bytes());
    format!("blink_android-{:x}", digest.finalize())
}

fn insert(headers: &mut HeaderMap, name: &'static str, value: &str) -> Result<(), BlinkError> {
    headers.insert(name, header(value)?);
    Ok(())
}

fn header(value: &str) -> Result<HeaderValue, BlinkError> {
    HeaderValue::from_str(value).map_err(|_| BlinkError::InvalidResponse)
}

#[cfg(test)]
mod tests {
    use super::{
        PROTOCOL, SignalingIdentity, canonical_ring_user_id, client_id, signaling_request,
    };
    use zeroize::Zeroizing;

    fn identity() -> SignalingIdentity {
        SignalingIdentity {
            token: Zeroizing::new("secret-shaped-token".into()),
            hardware_id: "hardware".into(),
            user_id: "user".into(),
        }
    }

    #[test]
    fn builds_the_native_blink_signaling_handshake_without_query_secrets() {
        let identity = identity();
        let request = signaling_request(&identity).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(request.uri().to_string(), super::SIGNALING_URL);
        assert_eq!(request.headers()["x-sig-api-version"], PROTOCOL);
        assert_eq!(request.headers()["x-sig-auth-type"], "blink_oauth");
        assert_eq!(request.headers()["x-sig-client-id"], client_id(&identity));
        assert!(!request.uri().to_string().contains(identity.token.as_str()));
    }

    #[test]
    fn extracts_the_shared_ring_user_identity_from_native_account_shapes() {
        assert_eq!(
            super::text_or_number(&serde_json::json!({"ringUserId": 42}), "ringUserId").as_deref(),
            Some("42")
        );
        assert_eq!(
            super::text_or_number(&serde_json::json!({"ring_user_id": "43"}), "ring_user_id")
                .as_deref(),
            Some("43")
        );
        assert_eq!(
            canonical_ring_user_id(&serde_json::json!({"ringUserId": 42})).as_deref(),
            Some("42")
        );
        assert!(canonical_ring_user_id(&serde_json::json!({"ringUserId": 0})).is_none());
        assert!(canonical_ring_user_id(&serde_json::json!({"user_id": 44})).is_none());
    }
}
