use std::path::Path;

use serde::Serialize;
use vistoda_provider_kit::private_file;

use crate::error::EngineError;

/// Atomic `0600` JSON write (lib-vistoda-provider-kit `private_file`).
pub fn atomic_write_json(path: &Path, value: &impl Serialize) -> Result<(), EngineError> {
    private_file::write_json(path, value).map_err(|_| EngineError::RecordingIo)
}
