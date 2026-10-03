"""Structural contract for motion events and USB-free motion recording (ADR 0012)."""

from pathlib import Path

ROOT = Path(__file__).parents[1]
COMPONENT = ROOT / "custom_components/blink_live_bridge"
ENGINE = ROOT / "addon/vistoda_blink_engine/src"


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def test_motion_settings_are_admin_gated_and_bounded() -> None:
    websocket = read(COMPONENT / "motion_websocket.py")
    assert '"blink_live_bridge/motion_recording/get"' in websocket
    assert '"blink_live_bridge/motion_recording/set"' in websocket
    assert "connection.user.is_admin" in websocket
    assert "vol.In((15, 30, 60))" in websocket and "vol.Length(max=32)" in websocket
    assert 'put_json("/v1/motion/recording", payload)' in websocket
    assert "async_register_motion_websocket(hass)" in read(COMPONENT / "__init__.py")


def test_motion_sensor_follows_the_fast_cached_motion_state() -> None:
    motion = read(COMPONENT / "motion.py")
    sensor = read(COMPONENT / "binary_sensor.py")
    assert "timedelta(seconds=15)" in motion and 'get_json("/v1/motion")' in motion
    assert "error.status == 404" in motion, "older providers keep the clip-based state"
    assert "self.runtime.motion.async_add_listener" in sensor
    assert '"last_motion_at", "event_type", "has_media"' in sensor
    # Entity identity is unchanged: the motion sensor keeps its key and name.
    assert 'Description("motion_detected", "Movimento"' in sensor


def test_engine_reads_native_v4_events_and_never_loops_on_live_views() -> None:
    media = read(ENGINE / "blink_media_v4.rs")
    poller = read(ENGINE / "motion.rs")
    assert "/api/v4/accounts/{}/media?start_time=" in media
    assert "Some(json!({}))" in media and "MAX_PAGES" in media
    assert '"liveview" | "snapshot"' in media
    assert 'text(item, "type").as_deref() != Some("event")' in media
    assert "Duration::from_secs(30)" in poller and "Duration::from_secs(900)" in poller
    assert "warmed" in poller, "the first poll must not record backlog events"


def test_motion_recordings_are_bounded_and_never_evict_manual_ones() -> None:
    recorder = read(ENGINE / "motion_recorder.rs")
    rolling = read(ENGINE / "recordings/motion.rs")
    settings = read(ENGINE / "motion_settings.rs")
    assert "network.armed == Some(true)" in recorder and "settings.selects" in recorder
    assert 'format!("motion-{event_id}")' in rolling
    assert "item.trigger.as_deref() == Some(MOTION_TRIGGER)" in rolling
    assert "const DURATIONS: [u64; 3] = [15, 30, 60];" in settings
    assert "enabled: false" in settings, "motion recording is opt-in"
    routes = read(ENGINE / "api_motion.rs")
    assert '"/v1/motion"' in routes and '"/v1/motion/recording"' in routes
