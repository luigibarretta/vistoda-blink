use super::{PROTOCOL, SignalingIdentity, canonical_ring_user_id, client_id, signaling_request};
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
