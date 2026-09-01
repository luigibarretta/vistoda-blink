"""Static contracts that prevent regression to the official Blink runtime."""

import json
from pathlib import Path

ROOT = Path(__file__).parents[1]
COMPONENT = ROOT / "custom_components/blink_live_bridge"
RUST = ROOT / "addon/vistoda_blink_engine/src"


def test_adapter_has_no_blinkpy_or_official_runtime_dependency() -> None:
    """Neither normal adapter code nor metadata may require official Blink."""
    manifest = json.loads((COMPONENT / "manifest.json").read_text())
    assert "blink" not in manifest.get("dependencies", [])
    for path in COMPONENT.rglob("*.py"):
        source = path.read_text()
        assert "import blinkpy" not in source
        assert "from blinkpy" not in source
        assert "loaded_blink_coordinator" not in source
    migration = (COMPONENT / "migration.py").read_text()
    assert "async_import_official_credentials" in migration
    assert "one-time migration" in migration
    assert 'async_entries("blink")' in migration
    assert "dict(entries[0].data)" in migration
    assert "runtime_data" not in migration
    assert "str(attributes[key])" in migration
    for path in COMPONENT.rglob("*.py"):
        if path.name != "migration.py":
            assert 'async_loaded_entries("blink")' not in path.read_text()


def test_rust_owns_every_blink_protocol_responsibility() -> None:
    """The supervised engine includes auth, state, commands, media and IMMI."""
    modules = {path.name for path in RUST.glob("*.rs")}
    assert {
        "oauth.rs",
        "credentials.rs",
        "blink_refresh.rs",
        "blink_commands.rs",
        "live.rs",
        "framing.rs",
    } <= modules
    api = (RUST / "api.rs").read_text()
    assert "/v1/enrollment/start" in api
    assert "/v1/state" in api
    assert "/v1/cameras/{alias}/commands" in api
    assert "/v1/cameras/{alias}/live.ts" in api
    assert "/upstream" not in api


def test_official_platform_and_service_parity_is_declared() -> None:
    """Keep all HA 2026.8.3 official surfaces during future refactors."""
    constants = (COMPONENT / "const.py").read_text()
    for platform in (
        "ALARM_CONTROL_PANEL",
        "BINARY_SENSOR",
        "CAMERA",
        "SENSOR",
        "SWITCH",
    ):
        assert f"Platform.{platform}" in constants
    services = (COMPONENT / "services.yaml").read_text()
    for name in ("record:", "trigger_camera:", "save_video:", "save_recent_clips:"):
        assert name in services


def test_addon_is_private_and_supervised() -> None:
    """The engine must expose only its private supervised app network."""
    config = (ROOT / "addon/vistoda_blink_engine/config.yaml").read_text()
    dockerfile = (ROOT / "addon/vistoda_blink_engine/Dockerfile").read_text()
    assert "startup: services" in config
    assert "8099/tcp: null" in config
    assert "hassio_api: true" in config
    assert "blink_live_bridge" in config
    assert "aarch64" in config
    assert "FROM alpine:3.22" in dockerfile
    assert "HEALTHCHECK" in dockerfile
    assert (
        "su-exec bridge:bridge" in (ROOT / "addon/vistoda_blink_engine/rootfs/run.sh").read_text()
    )
    runner = (ROOT / "addon/vistoda_blink_engine/rootfs/run.sh").read_text()
    bootstrap = (ROOT / "addon/vistoda_blink_engine/rootfs/vistoda-app-bootstrap.sh").read_text()
    assert "vistoda_publish_discovery" in runner
    assert "vistoda_supervisor_app_info" in runner
    assert "http://supervisor/discovery" in bootstrap
    assert "http://supervisor/addons/self/info" in bootstrap
    assert "workload-token" in runner
    assert "api_token" not in runner


def test_standalone_reauth_and_redacted_diagnostics_are_present() -> None:
    """Expired cloud authorization must not require the official integration."""
    flow = (COMPONENT / "config_flow.py").read_text()
    runtime = (COMPONENT / "runtime.py").read_text()
    diagnostics = (COMPONENT / "diagnostics.py").read_text()
    assert "async_step_reauth" in flow
    assert "VERSION = 1" in flow
    assert "async_update_reload_and_abort" in flow
    assert "ConfigEntryAuthFailed" in runtime
    assert '"serial"' in diagnostics
    assert '"media_url"' in diagnostics
    assert "async_redact_data" in diagnostics
