"""Repository and stable-contract tests for Vistoda Blink."""

import json
from pathlib import Path

ROOT = Path(__file__).parents[1]
COMPONENT = ROOT / "custom_components/blink_live_bridge"


def test_component_layout_and_identity() -> None:
    """Expose Vistoda branding while preserving the loaded HA domain."""
    manifest = json.loads((COMPONENT / "manifest.json").read_text())
    assert manifest["domain"] == "blink_live_bridge"
    assert manifest["name"] == "Vistoda Blink"
    assert manifest["version"] == "0.4.6"
    assert manifest["documentation"].endswith("/vistoda-blink")
    assert manifest["issue_tracker"].endswith("/vistoda-blink/issues")


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
