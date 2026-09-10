mod job;
mod model;
mod recovery;
mod storage;

pub use model::RecordingManifest;

use std::{
    collections::BTreeMap, fs, os::unix::fs::PermissionsExt, path::PathBuf, sync::Arc,
    time::Instant,
};

use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};

use crate::{error::EngineError, hub::EngineState};

#[derive(Default, Deserialize, Serialize)]
struct Journal {
    #[serde(default = "schema_version")]
    schema_version: u8,
    #[serde(default)]
    recordings: Vec<RecordingManifest>,
    #[serde(default)]
    idempotency: BTreeMap<String, String>,
}

#[derive(Default)]
pub(super) struct State {
    manifests: BTreeMap<String, RecordingManifest>,
    idempotency: BTreeMap<String, String>,
}

pub struct RecordingManager {
    pub(super) directory: PathBuf,
    journal: PathBuf,
    max_duration: u64,
    pub(super) max_bytes: u64,
    quota_bytes: u64,
    pub(super) state: Mutex<State>,
    tasks: RwLock<Vec<tokio::task::JoinHandle<()>>>,
}

impl RecordingManager {
    pub fn load(
        directory: PathBuf,
        max_duration: u64,
        max_bytes: u64,
        quota_bytes: u64,
    ) -> Result<Arc<Self>, EngineError> {
        fs::create_dir_all(&directory).map_err(|_| EngineError::RecordingIo)?;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
            .map_err(|_| EngineError::RecordingIo)?;
        let journal = directory.join("recordings.json");
        let mut state = recovery::load_state(&journal)?;
        recovery::recover(&directory, &mut state)?;
        let manager = Arc::new(Self {
            directory,
            journal,
            max_duration,
            max_bytes,
            quota_bytes,
            state: Mutex::new(state),
            tasks: RwLock::new(Vec::new()),
        });
        manager.persist_initial()?;
        Ok(manager)
    }

    pub async fn start(
        self: &Arc<Self>,
        engine: EngineState,
        camera: &str,
        duration_seconds: u64,
        request_id: &str,
    ) -> Result<RecordingManifest, EngineError> {
        if !(1..=self.max_duration).contains(&duration_seconds)
            || !(8..=128).contains(&request_id.len())
        {
            return Err(EngineError::RecordingInvalid);
        }
        if !engine
            .client()
            .state()
            .await
            .cameras
            .iter()
            .any(|item| item.alias == camera)
        {
            return Err(EngineError::CameraNotFound);
        }
        let mut state = self.state.lock().await;
        if let Some(existing_id) = state.idempotency.get(request_id) {
            return state
                .manifests
                .get(existing_id)
                .filter(|item| {
                    item.camera == camera && item.requested_duration_seconds == duration_seconds
                })
                .cloned()
                .ok_or(EngineError::RecordingInvalid);
        }
        if state.manifests.values().any(|item| {
            item.camera == camera && matches!(item.status.as_str(), "pending" | "recording")
        }) {
            return Err(EngineError::RecordingActive);
        }
        if recovery::spool_bytes(&self.directory)?.saturating_add(self.max_bytes) > self.quota_bytes
        {
            return Err(EngineError::RecordingCapacity);
        }
        let manifest = RecordingManifest::pending(camera, duration_seconds);
        state
            .idempotency
            .insert(request_id.to_owned(), manifest.recording_id.clone());
        state
            .manifests
            .insert(manifest.recording_id.clone(), manifest.clone());
        self.persist(&state)?;
        drop(state);
        let manager = Arc::clone(self);
        let id = manifest.recording_id.clone();
        let mut tasks = self.tasks.write().await;
        tasks.retain(|task| !task.is_finished());
        tasks.push(tokio::spawn(
            async move { manager.record(engine, id).await },
        ));
        Ok(manifest)
    }

    pub async fn list(&self) -> Vec<RecordingManifest> {
        let mut values: Vec<_> = self
            .state
            .lock()
            .await
            .manifests
            .values()
            .cloned()
            .collect();
        values.sort_by(|left, right| right.requested_at.cmp(&left.requested_at));
        values
    }

    pub fn storage_descriptor(&self) -> Result<serde_json::Value, EngineError> {
        let used_bytes = recovery::spool_bytes(&self.directory)?;
        Ok(serde_json::json!({
            "directory": self.directory.to_string_lossy(),
            "scope": "addon_private",
            "used_bytes": used_bytes,
            "quota_bytes": self.quota_bytes,
            "available_bytes": self.quota_bytes.saturating_sub(used_bytes)
        }))
    }

    pub async fn get(&self, id: &str) -> Option<RecordingManifest> {
        self.state.lock().await.manifests.get(id).cloned()
    }

    pub async fn media_path(&self, id: &str) -> Option<PathBuf> {
        let state = self.state.lock().await;
        let manifest = state.manifests.get(id)?;
        if manifest.status != "ready" {
            return None;
        }
        let path = self.directory.join(format!("{id}.ts"));
        path.is_file().then_some(path)
    }

    pub async fn acknowledge(&self, id: &str) -> Result<bool, EngineError> {
        let mut state = self.state.lock().await;
        let Some(manifest) = state.manifests.get(id) else {
            return Ok(false);
        };
        if matches!(manifest.status.as_str(), "pending" | "recording") {
            return Err(EngineError::RecordingActive);
        }
        let path = self.directory.join(format!("{id}.ts"));
        if path.exists() {
            fs::remove_file(path).map_err(|_| EngineError::RecordingIo)?;
            fs::File::open(&self.directory)
                .and_then(|directory| directory.sync_all())
                .map_err(|_| EngineError::RecordingIo)?;
        }
        state.manifests.remove(id);
        state.idempotency.retain(|_, value| value != id);
        self.persist(&state)?;
        Ok(true)
    }

    pub(super) async fn mark_recording(&self, id: &str) -> Result<(String, u64), EngineError> {
        let mut state = self.state.lock().await;
        let manifest = state
            .manifests
            .get_mut(id)
            .ok_or(EngineError::RecordingInvalid)?;
        manifest.status = "recording".into();
        let result = (manifest.camera.clone(), manifest.requested_duration_seconds);
        self.persist(&state)?;
        Ok(result)
    }

    pub(super) async fn mark_ready(
        &self,
        id: &str,
        started: Instant,
        started_at: String,
        bytes: u64,
        sha256: String,
    ) -> Result<(), EngineError> {
        let mut state = self.state.lock().await;
        let manifest = state
            .manifests
            .get_mut(id)
            .ok_or(EngineError::RecordingInvalid)?;
        manifest.status = "ready".into();
        manifest.started_at = Some(started_at);
        manifest.completed_at = Some(model::utc_now());
        manifest.actual_duration_seconds = Some(started.elapsed().as_secs_f64());
        manifest.bytes = Some(bytes);
        manifest.sha256 = Some(sha256);
        manifest.error_code = None;
        self.persist(&state)
    }

    fn persist_initial(&self) -> Result<(), EngineError> {
        let state = self
            .state
            .try_lock()
            .map_err(|_| EngineError::RecordingIo)?;
        self.persist(&state)
    }

    pub(super) fn persist(&self, state: &State) -> Result<(), EngineError> {
        storage::atomic_write_json(
            &self.journal,
            &Journal {
                schema_version: schema_version(),
                recordings: state.manifests.values().cloned().collect(),
                idempotency: state.idempotency.clone(),
            },
        )
    }
}

const fn schema_version() -> u8 {
    1
}
