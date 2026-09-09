use std::{fs, path::Path};

use super::{Journal, State};
use crate::error::EngineError;

pub(super) fn spool_bytes(directory: &Path) -> Result<u64, EngineError> {
    fs::read_dir(directory)
        .map_err(|_| EngineError::RecordingIo)?
        .try_fold(0_u64, |total, entry| {
            let entry = entry.map_err(|_| EngineError::RecordingIo)?;
            let bytes = if entry.path().extension().is_some_and(|value| value == "ts") {
                entry
                    .metadata()
                    .map_err(|_| EngineError::RecordingIo)?
                    .len()
            } else {
                0
            };
            Ok(total.saturating_add(bytes))
        })
}

pub(super) fn load_state(path: &Path) -> Result<State, EngineError> {
    if !path.exists() {
        return Ok(State::default());
    }
    let journal: Journal =
        serde_json::from_slice(&fs::read(path).map_err(|_| EngineError::RecordingIo)?)
            .map_err(|_| EngineError::RecordingIo)?;
    Ok(State {
        manifests: journal
            .recordings
            .into_iter()
            .map(|item| (item.recording_id.clone(), item))
            .collect(),
        idempotency: journal.idempotency,
    })
}

pub(super) fn recover(directory: &Path, state: &mut State) -> Result<(), EngineError> {
    for manifest in state.manifests.values_mut() {
        let path = directory.join(format!("{}.ts", manifest.recording_id));
        if matches!(manifest.status.as_str(), "pending" | "recording") {
            manifest.fail("interrupted");
        } else if manifest.status == "ready" && !path.is_file() {
            manifest.fail("missing_media");
            manifest.bytes = None;
            manifest.sha256 = None;
        }
    }
    for entry in fs::read_dir(directory).map_err(|_| EngineError::RecordingIo)? {
        let path = entry.map_err(|_| EngineError::RecordingIo)?.path();
        if path.extension().is_some_and(|value| value == "partial") {
            fs::remove_file(path).map_err(|_| EngineError::RecordingIo)?;
        }
    }
    Ok(())
}
