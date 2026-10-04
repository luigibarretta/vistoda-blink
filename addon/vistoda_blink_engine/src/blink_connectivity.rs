//! Device connectivity derived from Blink's free-form `status` strings.
//!
//! Blink reports `online`/`offline` for Sync Modules, Minis and doorbells and
//! `done` for battery cameras whose last check-in completed; the public
//! blinkpy client uses the same table. Any other value (for example a
//! transient command state) is unknown: it stays visible as the raw `status`
//! attribute and never marks a device offline.

#[must_use]
pub fn online(status: Option<&str>) -> Option<bool> {
    match status?.trim().to_ascii_lowercase().as_str() {
        "online" | "done" => Some(true),
        "offline" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::online;

    #[test]
    fn maps_only_known_blink_status_values() {
        assert_eq!(online(Some("online")), Some(true));
        assert_eq!(online(Some("done")), Some(true));
        assert_eq!(online(Some(" Online ")), Some(true));
        assert_eq!(online(Some("offline")), Some(false));
        assert_eq!(online(Some("OFFLINE")), Some(false));
    }

    #[test]
    fn unknown_or_missing_status_is_never_offline() {
        assert_eq!(online(None), None);
        assert_eq!(online(Some("")), None);
        assert_eq!(online(Some("busy")), None);
        assert_eq!(online(Some("updating")), None);
    }
}
