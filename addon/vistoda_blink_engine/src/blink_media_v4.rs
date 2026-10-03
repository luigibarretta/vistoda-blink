//! Native Blink Android 59.1 cloud event list (`POST v4/accounts/{id}/media`).

use serde::Serialize;
use serde_json::{Value, json};

use crate::blink_client::{BlinkClient, BlinkError};

/// Bounded pages per poll; the native client stops when no key is returned.
const MAX_PAGES: usize = 3;

/// One provider event, with or without video (`type == "event"`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MotionEvent {
    pub id: String,
    pub created_at: String,
    pub device_id: Option<String>,
    pub device_name: String,
    pub network_id: Option<String>,
    pub event_type: Option<String>,
    pub has_media: bool,
    pub no_media_reason: Option<String>,
}

impl BlinkClient {
    /// Read provider events created since `start_time` (ISO-8601), newest pages first.
    pub async fn motion_events(&self, start_time: &str) -> Result<Vec<MotionEvent>, BlinkError> {
        let context = self.context().await?;
        let mut events = Vec::new();
        let mut key: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let mut path = format!(
                "/api/v4/accounts/{}/media?start_time={}",
                context.account_id,
                url::form_urlencoded::byte_serialize(start_time.as_bytes()).collect::<String>()
            );
            if let Some(key) = &key {
                path.push_str("&pagination_key=");
                path.push_str(key);
            }
            let page = self.post_json(&context, &path, Some(json!({}))).await?;
            let (items, next) = parse_page(&page);
            events.extend(items);
            match next {
                Some(next) if key.as_deref() != Some(next.as_str()) => key = Some(next),
                _ => break,
            }
        }
        Ok(events)
    }
}

/// Parse one page; unknown fields are ignored and missing fields stay optional.
pub fn parse_page(value: &Value) -> (Vec<MotionEvent>, Option<String>) {
    let events = value
        .get("media")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(parse_event).collect())
        .unwrap_or_default();
    (events, id_text(value.get("pagination_key")))
}

fn parse_event(item: &Value) -> Option<MotionEvent> {
    let event_type = text(item, "event_type");
    // Live views and snapshots are user actions, including Vistoda's own
    // motion recordings; treating them as motion would loop forever.
    let source = text(item, "source");
    if matches!(event_type.as_deref(), Some("liveview" | "snapshot"))
        || source.as_deref() == Some("liveview")
    {
        return None;
    }
    let has_media = text(item, "type").as_deref() != Some("event")
        && text(item, "media").is_some_and(|media| !media.is_empty());
    Some(MotionEvent {
        id: id_text(item.get("id"))?,
        created_at: text(item, "created_at")?,
        device_id: id_text(item.get("device_id")),
        device_name: text(item, "device_name").unwrap_or_default(),
        network_id: id_text(item.get("network_id")),
        event_type,
        has_media,
        no_media_reason: text(item, "no_media_reason").filter(|reason| reason != "none"),
    })
}

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}

/// Blink sends numeric IDs; accept strings too and reject zero/empty values.
fn id_text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::Number(number) => number
            .as_u64()
            .filter(|id| *id > 0)
            .map(|id| id.to_string()),
        Value::String(text) if !text.is_empty() && text != "0" => Some(text.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::parse_page;

    #[test]
    fn keeps_video_less_events_and_skips_live_views() {
        let (events, next) = parse_page(&json!({
            "pagination_key": 99,
            "media": [
                {"id": 7, "created_at": "2026-10-03T16:45:00+00:00", "device_id": 11,
                 "device_name": "Balcone", "network_id": 85507, "type": "event",
                 "event_type": "pir", "no_media_reason": "no_subscription"},
                {"id": 8, "created_at": "2026-10-03T16:46:00+00:00", "device_name": "Cucina",
                 "type": "video", "event_type": "cv_motion", "media": "/api/v2/clip.mp4",
                 "no_media_reason": "none"},
                {"id": 9, "created_at": "2026-10-03T16:47:00+00:00", "event_type": "liveview"},
                {"id": 10, "created_at": "2026-10-03T16:48:00+00:00", "source": "liveview"},
                {"id": 0, "created_at": "2026-10-03T16:49:00+00:00"},
                {"id": 12}
            ]
        }));
        assert_eq!(next.as_deref(), Some("99"));
        assert_eq!(events.len(), 2);
        assert!(!events[0].has_media);
        assert_eq!(events[0].device_id.as_deref(), Some("11"));
        assert_eq!(
            events[0].no_media_reason.as_deref(),
            Some("no_subscription")
        );
        assert!(events[1].has_media);
        assert_eq!(events[1].no_media_reason, None);
    }
}
