"""Repository and stable-contract tests for Vistoda Blink."""

import json
from pathlib import Path

ROOT = Path(__file__).parents[1]
COMPONENT = ROOT / "custom_components/blink_live_bridge"
ENGINE = ROOT / "addon/vistoda_blink_engine"


def test_component_layout_and_identity() -> None:
    """Expose Vistoda branding while preserving the loaded HA domain."""
    manifest = json.loads((COMPONENT / "manifest.json").read_text())
    assert manifest["domain"] == "blink_live_bridge"
    assert manifest["name"] == "Vistoda Blink"
    assert manifest["version"] == "0.12.5"
    assert manifest["documentation"].endswith("/vistoda-blink")
    assert manifest["issue_tracker"].endswith("/vistoda-blink/issues")


def test_engine_installs_one_process_level_tls_provider() -> None:
    """Native signaling must not panic when reqwest and WebSocket TLS coexist."""
    source = (ENGINE / "src" / "main.rs").read_text()
    assert "ring::default_provider().install_default()" in source


def test_adapter_reuses_the_provider_bootstrap_state_on_first_refresh() -> None:
    runtime = (COMPONENT / "runtime.py").read_text(encoding="utf-8")

    assert "self._initial_update = True" in runtime
    assert 'cached = await self.client.get_json("/v1/state")' in runtime
    assert runtime.index("if self._initial_update") < runtime.index(
        'await self.client.post("/v1/refresh")'
    )


def test_vistoda_discovery_and_device_identity_stay_stable() -> None:
    """Prevent duplicate provider entries, devices and camera sessions."""
    setup = (COMPONENT / "__init__.py").read_text()
    constants = (COMPONENT / "const.py").read_text()
    assert 'VISTODA_DOMAIN = "media_bridge"' in constants
    assert 'data={"provider": "blink"}' in setup
    assert 'VISTODA_BLINK_IDENTIFIER = "blink:blink"' in constants


def test_device_hierarchy_uses_the_single_entry_registry_contract() -> None:
    """HA 2026.8+ requires an explicit parent ID instead of a global identifier."""
    setup = (COMPONENT / "__init__.py").read_text()
    runtime = (COMPONENT / "runtime.py").read_text()
    entity = (COMPONENT / "entity.py").read_text()
    alarm = (COMPONENT / "alarm_control_panel.py").read_text()

    assert "async_get_or_create(" in setup
    assert "config_entry_id=entry.entry_id" in setup
    assert "parent_device_id=parent_device.id" in setup
    assert "parent_device_id: str" in runtime
    assert "via_device_id=parent_device_id" in entity
    assert "via_device_id=runtime.parent_device_id" in alarm
    assert "via_device=" not in entity + alarm


def test_supervisor_discovery_removes_the_yaml_requirement() -> None:
    flow = (COMPONENT / "config_flow.py").read_text()
    setup = (COMPONENT / "__init__.py").read_text()
    client = (COMPONENT / "client.py").read_text()
    assert "async_step_hassio" in flow
    assert "CONF_MANAGED_APP: True" in flow
    assert "vol.Optional(DOMAIN)" in setup
    assert "entry.data.get(CONF_TOKEN)" in setup
    assert "base_url: str = ENGINE_URL" in client
    runner = (ROOT / "addon/vistoda_blink_engine/rootfs/run.sh").read_text()
    bootstrap = (ROOT / "addon/vistoda_blink_engine/rootfs/vistoda-app-bootstrap.sh").read_text()
    assert 'vistoda_prepare_data_dir bridge:bridge "${data_dir}"' in runner
    assert 'vistoda_secure_file bridge:bridge "${data_dir}/provider.sealed"' in runner
    assert "vistoda_prepare_data_dir" in bootstrap


def test_private_media_contract_stays_compatible() -> None:
    """Preserve both HA and SceneTrove MPEG-TS endpoint names."""
    source = (COMPONENT / "http.py").read_text()
    assert "live\\\\.(?:ts|mpegts)" in source
    assert '"video/mp2t"' in source
    assert "alias: str" in source
    assert "stream_format: str" in source


def test_camera_settings_boundary_is_redacted_and_admin_only() -> None:
    """The panel never receives provider credentials or an untyped write proxy."""
    source = (COMPONENT / "websocket.py").read_text()
    manifest = json.loads((COMPONENT / "manifest.json").read_text())
    assert "websocket_api" in manifest["dependencies"]
    assert "blink_live_bridge/camera/settings" in source
    assert "blink_live_bridge/camera/capabilities" in source
    assert "connection.user.is_admin" in source
    assert '("key", "value", "revision")' in source
    assert "api_token" not in source
    assert "Authorization" not in source


def test_zone_boundary_is_typed_bounded_and_admin_only() -> None:
    """Zone writes accept only the native v1 shape and never browser credentials."""
    source = (COMPONENT / "zones_websocket.py").read_text()
    setup = (COMPONENT / "__init__.py").read_text()
    assert "blink_live_bridge/camera/zones" in source
    assert "blink_live_bridge/camera/zones/update" in source
    assert "vol.Length(min=25, max=25)" in source
    assert "vol.Length(max=2)" in source
    assert "connection.user.is_admin" in source
    assert "async_register_zones_websocket(hass)" in setup
    assert "api_token" not in source
    assert "Authorization" not in source


def test_clip_services_refresh_provider_state_before_selection() -> None:
    """A newly recorded clip must not be hidden by stale coordinator data."""
    source = (COMPONENT / "camera.py").read_text()
    save_video = source.split("async def save_video", 1)[1].split("async def save_recent_clips", 1)[
        0
    ]
    save_recent = source.split("async def save_recent_clips", 1)[1].split("async def _command", 1)[
        0
    ]
    refresh = "await self.coordinator.async_request_refresh()"
    assert save_video.index(refresh) < save_video.index("self._camera_clips()")
    assert save_recent.index(refresh) < save_recent.index("self._camera_clips()")
    assert "if not clips:\n            return" in save_video


def test_local_recordings_replace_the_vendor_motion_clip_action() -> None:
    """UI recordings consume the live stream and never synthesize Blink motion."""
    setup = (COMPONENT / "__init__.py").read_text()
    camera = (COMPONENT / "camera.py").read_text()
    boundary = (COMPONENT / "recording_websocket.py").read_text()
    http = (COMPONENT / "http.py").read_text()
    api = (ROOT / "addon/vistoda_blink_engine/src/api_recordings.rs").read_text()
    assert "async_register_recording_websocket(hass)" in setup
    assert '"duration_seconds": 30' in camera and "uuid4()" in camera
    assert "blink_live_bridge/recordings/create" in boundary
    assert "connection.user.is_admin" in boundary
    assert "vol.In((15, 30, 60))" in boundary
    assert "RecordingMediaView" in http and "requires_auth = True" in http
    assert '"/v1/cameras/{alias}/recordings"' in api
    assert "api_token" not in boundary


def test_sync_module_usb_boundary_is_guarded_and_ha_authenticated() -> None:
    setup = (COMPONENT / "__init__.py").read_text()
    boundary = (COMPONENT / "storage_websocket.py").read_text()
    http = (COMPONENT / "http.py").read_text()
    provider = (ROOT / "addon/vistoda_blink_engine/src/api_storage.rs").read_text()
    assert "async_register_storage_websocket(hass)" in setup
    assert "blink_live_bridge/local_storage/list" in boundary
    assert "LocalStorageMediaView" in http and "requires_auth = True" in http
    assert '"/v1/local-storage"' in provider
    assert "api_token" not in boundary + http
    assert "blink_live_bridge/local_storage/delete" in boundary
    assert "blink_live_bridge/local_storage/format" in boundary
    assert "connection.user.is_admin" in boundary
    assert 'f"FORMATTA {msg[' in boundary
    assert "local_storage/eject" not in boundary + http + provider
    assert "local_storage/mount" not in boundary + http + provider


def test_camera_declares_the_official_blink_attribute_surface() -> None:
    """Preserve attributes used by dashboards during official-provider removal."""
    source = (COMPONENT / "camera.py").read_text()
    for name in (
        "camera_id",
        "last_record",
        "recent_clips",
        "sync_module",
        "temperature_c",
        "thumbnail",
        "video",
    ):
        assert f'"{name}"' in source


def test_webrtc_signaling_is_typed_owner_bound_and_media_free() -> None:
    """OAuth stays in Rust while HA owns one bounded browser subscription."""
    setup = (COMPONENT / "__init__.py").read_text()
    boundary = (COMPONENT / "webrtc_websocket.py").read_text()
    client = (COMPONENT / "client.py").read_text()
    engine = (ENGINE / "src" / "blink_webrtc.rs").read_text()
    commands = (ENGINE / "src" / "blink_webrtc_commands.rs").read_text()
    events = (ENGINE / "src" / "blink_webrtc_events.rs").read_text()
    wire = (ENGINE / "src" / "blink_webrtc_wire.rs").read_text()
    hub = (ENGINE / "src" / "hub.rs").read_text()
    assert "async_register_webrtc_websocket(hass)" in setup
    assert "blink_live_bridge/webrtc/subscribe" in boundary
    assert "connection.subscriptions" in boundary and "session.owner != id(connection)" in boundary
    assert boundary.index("sessions[handle] = session") < boundary.index("runtime.client.websocket")
    assert "relay_task" in boundary and "asyncio.timeout(5)" in boundary
    assert "max_msg_size=128 * 1024" in client
    assert '"activate_session"' in commands and '"mic_enable"' in commands
    assert '"stream_options"' in commands and '"camera_options"' in commands
    assert '"mlineindex"' in wire and "pub dialog_id" in wire
    assert "OWNER_WEBRTC" in hub and "EngineError::PublisherBusy" in hub
    assert "valid_event_session" in events and "mic_cooldown" in events
    assert "webrtc-rs" not in engine + commands + wire and "api_token" not in boundary
    assert "let relay_failed = ui(browser, event).await.is_err();" in events
    assert "closed || ui(browser, event).await.is_err()" not in events


def test_every_maintained_file_stays_bounded() -> None:
    """Keep every responsibility below the product LOC budget."""
    excluded = {".git", ".pytest_cache", ".ruff_cache", ".venv", "__pycache__"}
    suffixes = {".json", ".md", ".py", ".rs", ".toml", ".yaml", ".yml"}
    for path in ROOT.rglob("*"):
        if (
            path.is_file()
            and path.suffix in suffixes
            and not excluded.intersection(path.relative_to(ROOT).parts)
        ):
            assert len(path.read_text().splitlines()) <= 250, path
