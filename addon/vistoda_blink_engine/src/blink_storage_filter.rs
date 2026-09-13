use std::collections::BTreeSet;

use crate::blink_storage::LocalStorageClip;

pub fn apply(
    clips: &mut Vec<LocalStorageClip>,
    selected: Option<&BTreeSet<String>>,
) -> Vec<String> {
    let available = clips
        .iter()
        .map(|clip| clip.device_name.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if let Some(selected) = selected {
        clips.retain(|clip| selected.contains(&clip.device_name));
    }
    available
}
