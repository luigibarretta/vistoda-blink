use std::{fs, io::Write, os::unix::fs::OpenOptionsExt, path::Path};

use serde::Serialize;
use uuid::Uuid;

use crate::error::EngineError;

pub fn atomic_write_json(path: &Path, value: &impl Serialize) -> Result<(), EngineError> {
    let parent = path.parent().ok_or(EngineError::RecordingIo)?;
    let temporary = parent.join(format!(".recordings-{}.tmp", Uuid::new_v4()));
    let encoded = serde_json::to_vec(value).map_err(|_| EngineError::RecordingIo)?;
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(&encoded)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        fs::File::open(parent)?.sync_all()
    })();
    if result.is_err() {
        let _ignored = fs::remove_file(&temporary);
    }
    result.map_err(|_| EngineError::RecordingIo)
}
