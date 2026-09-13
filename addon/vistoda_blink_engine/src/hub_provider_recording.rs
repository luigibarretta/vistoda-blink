use super::{EngineState, OWNER_IMMI};
use crate::{error::EngineError, immi_audio_lease::AudioStatus};
use tokio::sync::watch;

impl EngineState {
    pub(super) async fn camera_exists(&self, alias: &str) -> bool {
        self.client
            .state()
            .await
            .cameras
            .iter()
            .any(|camera| camera.alias == alias)
    }

    pub async fn request_provider_recording(
        &self,
        alias: &str,
        save: bool,
    ) -> Result<watch::Receiver<AudioStatus>, EngineError> {
        if !self.camera_exists(alias).await {
            return Err(EngineError::CameraNotFound);
        }
        let hub = self.hub(alias).await;
        if hub.owner() != OWNER_IMMI {
            return Err(EngineError::ProviderRecordingUnavailable);
        }
        hub.audio
            .request_recording(save)
            .map_err(|_| EngineError::ProviderRecordingUnavailable)
    }
}
