use serde_json::json;

use crate::blink_client::BlinkError;

use super::{
    CameraZonesUpdate, MICRO_MASK, PrivacyZone, apply_privacy, parse_zones, update_body,
    validated_update,
};

fn response() -> serde_json::Value {
    json!({
        "motion_regions": 33_554_431,
        "advanced_motion_regions": vec![4095; 25],
        "privacy_zones": []
    })
}

#[test]
fn parses_exact_native_v1_grid_without_leaking_unknown_fields()
-> Result<(), Box<dyn std::error::Error>> {
    let mut raw = response();
    raw["account_secret"] = json!("never expose");
    let zones = parse_zones("balcone", &raw, true)?;
    assert_eq!(zones.basic_columns, 5);
    assert_eq!(zones.micro_rows, 3);
    assert_eq!(zones.activity_masks, vec![MICRO_MASK; 25]);
    assert!(!serde_json::to_string(&zones)?.contains("account_secret"));
    assert_eq!(zones.revision.len(), 64);
    Ok(())
}

#[test]
fn privacy_spans_clear_the_matching_twenty_by_fifteen_micro_cells() {
    let mut masks = vec![MICRO_MASK; 25];
    apply_privacy(
        &mut masks,
        &[PrivacyZone {
            x: 3,
            y: 2,
            w: 2,
            h: 2,
        }],
    );
    assert_eq!(masks[0] & (1 << 11), 0);
    assert_eq!(masks[1] & (1 << 8), 0);
    assert_eq!(masks[5] & (1 << 3), 0);
    assert_eq!(masks[6] & 1, 0);
}

#[test]
fn update_preserves_high_provider_bits_and_derives_activity_bits() -> Result<(), BlinkError> {
    let input = CameraZonesUpdate {
        revision: "a".repeat(64),
        activity_masks: vec![MICRO_MASK; 25],
        privacy_zones: Vec::new(),
    };
    let before = parse_zones("balcone", &response(), true)?;
    let mut desired = validated_update(&before, &input)?;
    desired.activity_masks[1] = 0;
    let original = json!({
        "motion_regions": (1_u64 << 28) | (1_u64 << 29) | 33_554_431,
        "advanced_motion_regions": vec![4095; 25],
        "privacy_zones": []
    });
    let body = update_body(&original, &desired)?;
    let motion = body["motion_regions"]
        .as_u64()
        .ok_or(BlinkError::InvalidResponse)?;
    assert_eq!(motion & (1 << 1), 0);
    assert_ne!(motion & (1 << 28), 0);
    assert_ne!(motion & (1 << 29), 0);
    assert_ne!(motion & (1 << 30), 0);
    Ok(())
}

#[test]
fn rejects_out_of_grid_privacy_and_an_all_disabled_grid() {
    let before = parse_zones("balcone", &response(), true).ok();
    let invalid_zone = CameraZonesUpdate {
        revision: "a".repeat(64),
        activity_masks: vec![MICRO_MASK; 25],
        privacy_zones: vec![PrivacyZone {
            x: 19,
            y: 14,
            w: 2,
            h: 1,
        }],
    };
    assert!(
        before
            .as_ref()
            .is_some_and(|zones| validated_update(zones, &invalid_zone).is_err())
    );
    let disabled = CameraZonesUpdate {
        revision: "a".repeat(64),
        activity_masks: vec![0; 25],
        privacy_zones: Vec::new(),
    };
    assert!(
        before
            .as_ref()
            .is_some_and(|zones| validated_update(zones, &disabled).is_err())
    );
}
