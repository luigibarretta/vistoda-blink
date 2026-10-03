//! Administrator settings for USB-free motion recording, persisted in `/data`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{error::EngineError, recordings::storage::atomic_write_json};

/// Native-length choices shared with the HA-local REC menu.
const DURATIONS: [u64; 3] = [15, 30, 60];
const MAX_CAMERAS: usize = 32;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MotionRecordingSettings {
    pub enabled: bool,
    pub duration_seconds: u64,
    /// Selected camera aliases; empty means every camera.
    pub cameras: Vec<String>,
}

impl Default for MotionRecordingSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            duration_seconds: 30,
            cameras: Vec::new(),
        }
    }
}

impl MotionRecordingSettings {
    pub fn validate(&self) -> Result<(), EngineError> {
        let aliases_valid = self.cameras.len() <= MAX_CAMERAS
            && self
                .cameras
                .iter()
                .all(|alias| crate::api::validate_alias(alias).is_ok());
        if DURATIONS.contains(&self.duration_seconds) && aliases_valid {
            Ok(())
        } else {
            Err(EngineError::RecordingInvalid)
        }
    }

    pub fn selects(&self, alias: &str) -> bool {
        self.cameras.is_empty() || self.cameras.iter().any(|item| item == alias)
    }
}

/// File-backed settings; a missing or unreadable file means the safe default (off).
pub struct MotionSettingsStore {
    path: PathBuf,
}

impl MotionSettingsStore {
    pub fn new(credentials_path: &Path) -> Self {
        Self {
            path: credentials_path.with_file_name("motion-recording.json"),
        }
    }

    pub fn load(&self) -> MotionRecordingSettings {
        std::fs::read(&self.path)
            .ok()
            .and_then(|payload| serde_json::from_slice::<MotionRecordingSettings>(&payload).ok())
            .filter(|settings| settings.validate().is_ok())
            .unwrap_or_default()
    }

    pub fn save(&self, settings: &MotionRecordingSettings) -> Result<(), EngineError> {
        settings.validate()?;
        atomic_write_json(&self.path, settings)
    }
}

impl crate::hub::EngineState {
    pub fn motion_settings(&self) -> &MotionSettingsStore {
        &self.motion_settings
    }
}

#[cfg(test)]
mod tests {
    use super::{MotionRecordingSettings, MotionSettingsStore};

    #[test]
    fn validates_durations_aliases_and_selection() {
        let mut settings = MotionRecordingSettings::default();
        assert!(!settings.enabled && settings.validate().is_ok() && settings.selects("cucina"));
        settings.duration_seconds = 45;
        assert!(settings.validate().is_err());
        settings.duration_seconds = 60;
        settings.cameras = vec!["balcone".into()];
        assert!(settings.validate().is_ok());
        assert!(settings.selects("balcone") && !settings.selects("cucina"));
        settings.cameras = vec!["../bad".into()];
        assert!(settings.validate().is_err());
    }

    #[test]
    fn round_trips_and_defaults_safely() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let store = MotionSettingsStore::new(&directory.path().join("credentials.json"));
        assert_eq!(store.load(), MotionRecordingSettings::default());
        let settings = MotionRecordingSettings {
            enabled: true,
            duration_seconds: 15,
            cameras: vec!["cucina".into()],
        };
        store.save(&settings)?;
        assert_eq!(store.load(), settings);
        std::fs::write(directory.path().join("motion-recording.json"), b"{broken")?;
        assert_eq!(store.load(), MotionRecordingSettings::default());
        Ok(())
    }
}
