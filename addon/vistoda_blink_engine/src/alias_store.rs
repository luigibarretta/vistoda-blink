use std::{collections::BTreeMap, path::PathBuf};

use tokio::io::AsyncWriteExt;

use crate::{blink_model::CameraState, credentials::StoreError};

#[derive(Clone)]
pub struct AliasStore {
    path: PathBuf,
}

impl AliasStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub async fn reconcile(&self, cameras: &mut [CameraState]) -> Result<(), StoreError> {
        let previous = self.load().await?;
        let mut aliases = BTreeMap::new();
        let mut used: Vec<String> = Vec::new();
        for camera in cameras {
            let preferred = previous
                .get(&camera.id)
                .filter(|alias| valid(alias) && !used.contains(*alias))
                .cloned()
                .unwrap_or_else(|| unique(&camera.alias, &used));
            used.push(preferred.clone());
            camera.alias.clone_from(&preferred);
            aliases.insert(camera.id.clone(), preferred);
        }
        if aliases != previous {
            self.save(&aliases).await?;
        }
        Ok(())
    }

    async fn load(&self) -> Result<BTreeMap<String, String>, StoreError> {
        match tokio::fs::read(&self.path).await {
            Ok(payload) => Ok(serde_json::from_slice(&payload)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(error) => Err(error.into()),
        }
    }

    async fn save(&self, aliases: &BTreeMap<String, String>) -> Result<(), StoreError> {
        let temporary = self.path.with_extension("json.new");
        let mut file = tokio::fs::File::create(&temporary).await?;
        file.write_all(&serde_json::to_vec(aliases)?).await?;
        file.sync_all().await?;
        drop(file);
        tokio::fs::rename(&temporary, &self.path).await?;
        if let Some(parent) = self.path.parent() {
            tokio::fs::File::open(parent).await?.sync_all().await?;
        }
        Ok(())
    }
}

fn valid(alias: &str) -> bool {
    !alias.is_empty()
        && alias.len() <= 64
        && alias
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_-".contains(&byte))
}

fn unique(preferred: &str, used: &[String]) -> String {
    let base = preferred.chars().take(54).collect::<String>();
    let base = if valid(&base) { base } else { "camera".into() };
    if !used.contains(&base) {
        return base;
    }
    let mut suffix = 2_u64;
    loop {
        let candidate = format!("{base}_{suffix}");
        if !used.contains(&candidate) {
            return candidate;
        }
        suffix = suffix.saturating_add(1);
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::AliasStore;
    use crate::blink_model::CameraState;

    fn camera() -> CameraState {
        CameraState {
            id: "2".into(),
            network_id: "1".into(),
            alias: "balcone".into(),
            name: "Balcone".into(),
            serial: None,
            firmware: None,
            camera_type: "default".into(),
            product_type: "catalina".into(),
            enabled: None,
            status: None,
            battery_state: None,
            battery_voltage: None,
            battery_level: None,
            low_battery: None,
            temperature_f: None,
            wifi_dbm: None,
            motion_detected: false,
            thumbnail_url: None,
            powered: false,
            ring_device_id: None,
            two_way_audio: None,
            audio_aec: None,
            audio_privacy_enabled: None,
        }
    }

    #[tokio::test]
    async fn preserves_aliases_when_a_camera_is_renamed() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let store = AliasStore::new(directory.path().join("camera-aliases.json"));
        let mut original = vec![camera()];
        store.reconcile(&mut original).await.expect("seed aliases");
        let mut renamed = vec![camera()];
        renamed[0].name = "Terrazzo".into();
        renamed[0].alias = "terrazzo".into();
        store.reconcile(&mut renamed).await.expect("restore alias");
        assert_eq!(renamed[0].alias, "balcone");
    }
}
