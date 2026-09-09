use std::fmt::Write;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::blink_client::BlinkError;

const BASIC_ROWS: usize = 5;
const BASIC_COLUMNS: usize = 5;
const MICRO_ROWS: usize = 3;
const MICRO_COLUMNS: usize = 4;
const MICRO_MASK: u16 = 0x0fff;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PrivacyZone {
    pub x: u8,
    pub y: u8,
    pub w: u8,
    pub h: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CameraZones {
    pub alias: String,
    pub revision: String,
    pub privacy_supported: bool,
    pub basic_rows: usize,
    pub basic_columns: usize,
    pub micro_rows: usize,
    pub micro_columns: usize,
    pub activity_masks: Vec<u16>,
    pub privacy_zones: Vec<PrivacyZone>,
}

#[derive(Debug, Deserialize)]
pub struct CameraZonesUpdate {
    pub revision: String,
    pub activity_masks: Vec<u16>,
    pub privacy_zones: Vec<PrivacyZone>,
}

pub fn parse_zones(
    alias: &str,
    raw: &Value,
    privacy_supported: bool,
) -> Result<CameraZones, BlinkError> {
    let object = raw.as_object().ok_or(BlinkError::InvalidResponse)?;
    let masks = object
        .get("advanced_motion_regions")
        .and_then(Value::as_array)
        .ok_or(BlinkError::InvalidResponse)?;
    if masks.len() != BASIC_ROWS * BASIC_COLUMNS {
        return Err(BlinkError::InvalidResponse);
    }
    let activity_masks = masks
        .iter()
        .map(|value| {
            let mask = value.as_u64().ok_or(BlinkError::InvalidResponse)?;
            u16::try_from(mask)
                .ok()
                .filter(|item| *item <= MICRO_MASK)
                .ok_or(BlinkError::InvalidResponse)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let privacy_zones = match object.get("privacy_zones") {
        None | Some(Value::Null) => Vec::new(),
        Some(value) => {
            serde_json::from_value(value.clone()).map_err(|_| BlinkError::InvalidResponse)?
        }
    };
    validate_privacy(&privacy_zones)?;
    Ok(CameraZones {
        alias: alias.to_owned(),
        revision: revision(raw),
        privacy_supported,
        basic_rows: BASIC_ROWS,
        basic_columns: BASIC_COLUMNS,
        micro_rows: MICRO_ROWS,
        micro_columns: MICRO_COLUMNS,
        activity_masks,
        privacy_zones,
    })
}

pub fn validated_update(
    before: &CameraZones,
    input: &CameraZonesUpdate,
) -> Result<CameraZones, BlinkError> {
    if input.activity_masks.len() != BASIC_ROWS * BASIC_COLUMNS
        || input.activity_masks.iter().any(|mask| *mask > MICRO_MASK)
    {
        return Err(BlinkError::InvalidSetting);
    }
    validate_privacy(&input.privacy_zones)?;
    if !before.privacy_supported && input.privacy_zones != before.privacy_zones {
        return Err(BlinkError::SettingsUnsupported);
    }
    let mut masks = input.activity_masks.clone();
    apply_privacy(&mut masks, &input.privacy_zones);
    if masks.iter().all(|mask| *mask == 0) {
        return Err(BlinkError::InvalidSetting);
    }
    Ok(CameraZones {
        alias: before.alias.clone(),
        revision: input.revision.clone(),
        privacy_supported: before.privacy_supported,
        basic_rows: BASIC_ROWS,
        basic_columns: BASIC_COLUMNS,
        micro_rows: MICRO_ROWS,
        micro_columns: MICRO_COLUMNS,
        activity_masks: masks,
        privacy_zones: input.privacy_zones.clone(),
    })
}

fn validate_privacy(zones: &[PrivacyZone]) -> Result<(), BlinkError> {
    const WIDTH: u16 = 20;
    const HEIGHT: u16 = 15;
    if zones.len() > 2
        || zones.iter().any(|zone| {
            zone.w == 0
                || zone.h == 0
                || u16::from(zone.x) + u16::from(zone.w) > WIDTH
                || u16::from(zone.y) + u16::from(zone.h) > HEIGHT
        })
    {
        return Err(BlinkError::InvalidSetting);
    }
    Ok(())
}

fn apply_privacy(masks: &mut [u16], zones: &[PrivacyZone]) {
    for zone in zones {
        for y in zone.y..zone.y + zone.h {
            for x in zone.x..zone.x + zone.w {
                let basic =
                    usize::from(y) / MICRO_ROWS * BASIC_COLUMNS + usize::from(x) / MICRO_COLUMNS;
                let bit =
                    usize::from(y) % MICRO_ROWS * MICRO_COLUMNS + usize::from(x) % MICRO_COLUMNS;
                masks[basic] &= !(1_u16 << bit);
            }
        }
    }
}

pub fn provider_body(raw: &Value) -> Result<Value, BlinkError> {
    let object = raw.as_object().ok_or(BlinkError::InvalidResponse)?;
    Ok(json!({
        "motion_regions": object.get("motion_regions").ok_or(BlinkError::InvalidResponse)?,
        "advanced_motion_regions": object.get("advanced_motion_regions")
            .ok_or(BlinkError::InvalidResponse)?,
        "privacy_zones": object.get("privacy_zones").cloned().unwrap_or(Value::Null),
    }))
}

pub fn update_body(original: &Value, desired: &CameraZones) -> Result<Value, BlinkError> {
    let old_motion = original
        .get("motion_regions")
        .and_then(Value::as_u64)
        .ok_or(BlinkError::InvalidResponse)?;
    let mut motion = old_motion & !((1_u64 << 25) - 1) & !(1_u64 << 30);
    for (index, mask) in desired.activity_masks.iter().enumerate() {
        if *mask != 0 {
            motion |= 1_u64 << index;
        }
    }
    if desired
        .activity_masks
        .iter()
        .any(|mask| *mask != MICRO_MASK)
    {
        motion |= 1_u64 << 30;
    }
    Ok(json!({
        "motion_regions": motion,
        "advanced_motion_regions": desired.activity_masks,
        "privacy_zones": desired.privacy_zones,
    }))
}

pub fn same_values(left: &CameraZones, right: &CameraZones) -> bool {
    left.activity_masks == right.activity_masks && left.privacy_zones == right.privacy_zones
}

fn revision(raw: &Value) -> String {
    let digest = Sha256::digest(serde_json::to_vec(raw).unwrap_or_default());
    digest
        .iter()
        .fold(String::with_capacity(64), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        })
}

#[cfg(test)]
#[path = "zones_tests.rs"]
mod tests;
