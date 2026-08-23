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
    assert manifest["version"] == "0.3.4"
    assert manifest["documentation"].endswith("/vistoda-blink")
    assert manifest["issue_tracker"].endswith("/vistoda-blink/issues")


def test_vistoda_discovery_and_device_identity_stay_stable() -> None:
    """Prevent duplicate provider entries, devices and camera sessions."""
    setup = (COMPONENT / "__init__.py").read_text()
    constants = (COMPONENT / "const.py").read_text()
    assert 'VISTODA_DOMAIN = "media_bridge"' in constants
    assert 'data={"provider": "blink"}' in setup
    assert 'VISTODA_BLINK_IDENTIFIER = "blink:blink"' in constants


def test_private_media_contract_stays_compatible() -> None:
    """Preserve both HA and SceneTrove MPEG-TS endpoint names."""
    source = (COMPONENT / "http.py").read_text()
    assert "live\\\\.(?:ts|mpegts)" in source
    assert '"video/mp2t"' in source
    assert "alias: str" in source
    assert "stream_format: str" in source


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
