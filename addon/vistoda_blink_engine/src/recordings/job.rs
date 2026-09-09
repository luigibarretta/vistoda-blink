use std::{
    fmt::Write as _,
    sync::Arc,
    time::{Duration, Instant},
};

use sha2::{Digest, Sha256};
use tokio::{
    fs::{self, OpenOptions},
    io::AsyncWriteExt,
    time::timeout,
};

use super::{RecordingManager, model::utc_now};
use crate::{error::EngineError, hub::EngineState, hub::HubMessage};

impl RecordingManager {
    pub(super) async fn record(self: Arc<Self>, engine: EngineState, id: String) {
        let result = self.capture(&engine, &id).await;
        let mut state = self.state.lock().await;
        let Some(manifest) = state.manifests.get_mut(&id) else {
            return;
        };
        if let Err(error) = result {
            let code = match error {
                EngineError::RecordingCapacity => "capacity",
                EngineError::RecordingInvalid => "invalid_media",
                _ => "upstream_failure",
            };
            manifest.fail(code);
        }
        if self.persist(&state).is_err() {
            tracing::error!(
                error_type = "recording_journal",
                "recording journal persist failed"
            );
        }
    }

    async fn capture(&self, engine: &EngineState, id: &str) -> Result<(), EngineError> {
        let (camera, duration) = self.mark_recording(id).await?;
        let mut subscriber = engine.subscribe(&camera).await?;
        let partial = self.directory.join(format!(".{id}.partial"));
        let final_path = self.directory.join(format!("{id}.ts"));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&partial)
            .await
            .map_err(|_| EngineError::RecordingIo)?;
        let work = async {
            let mut digest = Sha256::new();
            let mut written = 0_u64;
            let mut started = None;
            let mut started_at = None;
            loop {
                let frame = match subscriber.recv().await {
                    Ok(HubMessage::Data(frame)) => frame,
                    Ok(HubMessage::End) | Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        break;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                };
                if frame.first() != Some(&0x47) {
                    return Err(EngineError::RecordingInvalid);
                }
                let next = written.saturating_add(frame.len() as u64);
                if next > self.max_bytes {
                    return Err(EngineError::RecordingCapacity);
                }
                if started.is_none() {
                    started = Some(Instant::now());
                    started_at = Some(utc_now());
                }
                file.write_all(&frame)
                    .await
                    .map_err(|_| EngineError::RecordingIo)?;
                digest.update(&frame);
                written = next;
                if started.is_some_and(|instant| instant.elapsed() >= Duration::from_secs(duration))
                {
                    break;
                }
            }
            let started = started.ok_or(EngineError::RecordingInvalid)?;
            file.sync_all()
                .await
                .map_err(|_| EngineError::RecordingIo)?;
            drop(file);
            fs::rename(&partial, &final_path)
                .await
                .map_err(|_| EngineError::RecordingIo)?;
            std::fs::File::open(&self.directory)
                .and_then(|directory| directory.sync_all())
                .map_err(|_| EngineError::RecordingIo)?;
            Ok((
                started,
                started_at.unwrap_or_else(utc_now),
                written,
                digest.finalize(),
            ))
        };
        let result = match timeout(Duration::from_secs(duration + 45), work).await {
            Ok(result) => result,
            Err(_) => Err(EngineError::RecordingInvalid),
        };
        let (started, started_at, written, digest) = match result {
            Ok(value) => value,
            Err(error) => {
                let _ignored = fs::remove_file(&partial).await;
                return Err(error);
            }
        };
        let mut hash = String::with_capacity(64);
        for byte in digest {
            write!(&mut hash, "{byte:02x}").map_err(|_| EngineError::RecordingIo)?;
        }
        self.mark_ready(id, started, started_at, written, hash)
            .await
    }
}
