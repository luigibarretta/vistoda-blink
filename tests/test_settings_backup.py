"""Behavioral tests for fail-closed Blink settings restore helpers."""

import importlib.util
import sys
import types
from pathlib import Path

import pytest

ROOT = Path(__file__).parents[1]
COMPONENT = ROOT / "custom_components/blink_live_bridge"
PACKAGE = "backup_test_package"


class EngineError(Exception):
    def __init__(self, message: str, status: int | None = None) -> None:
        super().__init__(message)
        self.status = status


def _load(name: str):
    package = sys.modules.setdefault(PACKAGE, types.ModuleType(PACKAGE))
    package.__path__ = [str(COMPONENT)]
    client = sys.modules.setdefault(f"{PACKAGE}.client", types.ModuleType(f"{PACKAGE}.client"))
    client.EngineError = EngineError
    runtime = sys.modules.setdefault(f"{PACKAGE}.runtime", types.ModuleType(f"{PACKAGE}.runtime"))
    runtime.BridgeRuntime = object
    spec = importlib.util.spec_from_file_location(f"{PACKAGE}.{name}", COMPONENT / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


model = _load("settings_backup_model")
restore = _load("settings_restore")


def _field(key: str, value, writable: bool = True) -> dict:
    return {"key": key, "value": value, "writable": writable}


def test_stable_identity_never_falls_back_to_a_reused_alias() -> None:
    cameras = [
        {"id": "9", "serial": "new", "alias": "balcone"},
        {"id": "2", "serial": "wanted", "alias": "renamed"},
    ]
    assert (
        model.match_camera(cameras, {"camera_id": "2", "serial": "wanted", "alias": "balcone"})
        == cameras[1]
    )
    assert (
        model.match_camera(cameras, {"camera_id": "1", "serial": "missing", "alias": "balcone"})
        is None
    )


def test_preflight_rejects_irreversible_temperature_and_missing_fields() -> None:
    with pytest.raises(EngineError) as temperature:
        restore.restore_plan([_field("temperature_min", None)], [_field("temperature_min", 40)])
    assert temperature.value.status == 422
    with pytest.raises(EngineError):
        restore.restore_plan([_field("motion_detection", True)], [_field("removed", False)])


def test_preflight_ignores_uninitialized_temperature_no_ops() -> None:
    current = [
        _field("temperature_alerts", False),
        _field("temperature_min", None),
        _field("temperature_max", None),
        _field("motion_detection", True),
    ]
    desired = [
        _field("temperature_alerts", False),
        _field("temperature_min", None),
        _field("temperature_max", None),
        _field("motion_detection", False),
    ]
    assert restore.restore_plan(current, desired) == [("motion_detection", False)]


def test_readback_requires_every_backed_up_field() -> None:
    assert (
        restore.values_match([_field("temperature_min", None)], [_field("missing", None)]) is False
    )


@pytest.mark.asyncio
async def test_revision_conflict_performs_no_write() -> None:
    class Client:
        def __init__(self):
            self.posts = []

        async def get_json(self, _path):
            return {"revision": "new", "settings": [_field("motion_detection", True)]}

        async def post(self, path, payload):
            self.posts.append((path, payload))

    runtime = types.SimpleNamespace(client=Client())
    with pytest.raises(restore.CameraApplyError) as conflict:
        await restore.apply_camera(runtime, "balcone", [_field("motion_detection", False)], "old")
    assert conflict.value.status == 409
    assert conflict.value.changed is False
    assert runtime.client.posts == []


@pytest.mark.asyncio
async def test_restore_uses_returned_revision_and_verifies_values() -> None:
    class Client:
        def __init__(self):
            self.current = True
            self.revision = "one"
            self.posts = []

        async def get_json(self, _path):
            return {
                "revision": self.revision,
                "settings": [_field("motion_detection", self.current)],
            }

        async def post(self, path, payload):
            assert payload["revision"] == "one"
            self.posts.append((path, payload))
            self.current = payload["value"]
            self.revision = "two"
            return await self.get_json(path)

    runtime = types.SimpleNamespace(client=Client())
    changed, revision = await restore.apply_camera(
        runtime, "balcone", [_field("motion_detection", False)], "one"
    )
    assert changed is True
    assert revision == "two"
    assert len(runtime.client.posts) == 1
