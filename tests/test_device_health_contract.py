"""Structural contract for device health entities and USB polling (ADR 0013)."""

import json
from pathlib import Path

COMPONENT = Path(__file__).parents[1] / "custom_components/blink_live_bridge"


def read(name: str) -> str:
    return (COMPONENT / name).read_text(encoding="utf-8")


def test_entities_keep_existing_keys_and_add_new_ones() -> None:
    binary = read("binary_sensor.py")
    for existing in (
        '"low_battery"',
        '"enabled"',
        '"motion_detected"',
        '"temperature_out_of_range"',
    ):
        assert existing in binary
    assert (
        'Description("online", "Connessione", BinarySensorDeviceClass.CONNECTIVITY, True)' in binary
    )
    assert "network_binary_sensors(runtime)" in binary
    assert "network_sensors(runtime)" in read("sensor.py")
    health = read("network_health.py")
    for key, name in (
        ('"online"', '"Connessione"'),
        ('"usb_storage"', '"Archivio USB"'),
        ('"usb_storage_problem"', '"Problema archivio USB"'),
    ):
        assert key in health and name in health
    assert "BinarySensorDeviceClass.PROBLEM" in health
    entity = read("entity.py")
    assert 'f"vistoda-{serial}-{suffix}"' in entity
    assert 'network.get("has_sync_module")' in entity


def test_usb_status_is_polled_boundedly_and_after_commands() -> None:
    storage = read("storage_status.py")
    assert "timedelta(minutes=5)" in storage
    assert 'get_json("/v1/local-storage/status", STORAGE_TIMEOUT)' in storage
    setup = read("__init__.py")
    assert 'network.get("has_sync_module")' in setup and "storage=storage" in setup
    assert "await refresh_storage_entities(runtime)" in read("storage_command_websocket.py")
    assert "await refresh_storage_entities(runtime)" in read("storage_websocket.py")


def test_bridge_token_repair_issue_is_translated() -> None:
    for name in ("strings.json", "translations/it.json"):
        issues = json.loads(read(name))["issues"]
        assert set(issues["bridge_token_rejected"]) == {"title", "description"}
