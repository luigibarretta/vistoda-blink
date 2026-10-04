"""Connectivity, USB health and Blink re-login contracts (ADR 0013)."""

import ast
import importlib.util
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import AsyncMock, Mock

import aiohttp
import pytest

ROOT = Path(__file__).parents[1]
COMPONENT = ROOT / "custom_components/blink_live_bridge"


def read(name: str) -> str:
    return (COMPONENT / name).read_text(encoding="utf-8")


def load(name: str, namespace: dict, strip: set[str]) -> dict:
    """Execute a module's classes and functions without a Home Assistant install."""
    tree = ast.parse(read(name))
    nodes = []
    for node in tree.body:
        if isinstance(node, ast.ClassDef):
            node.decorator_list = []
            if node.name in strip:
                node.bases, node.keywords = [], []
            nodes.append(node)
        elif isinstance(node, ast.FunctionDef | ast.AsyncFunctionDef):
            nodes.append(node)
    future = ast.ImportFrom(module="__future__", names=[ast.alias(name="annotations")], level=0)
    module = ast.Module(body=[future, *nodes], type_ignores=[])
    exec(compile(ast.fix_missing_locations(module), name, "exec"), namespace)
    return namespace


def usb():
    spec = importlib.util.spec_from_file_location("usb_health", COMPONENT / "usb_health.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def client_module() -> dict:
    return load(
        "client.py",
        {
            "ClientError": aiohttp.ClientError,
            "ClientTimeout": aiohttp.ClientTimeout,
            "ENGINE_URL": "",
        },
        set(),
    )


ENGINE = client_module()
EngineError = ENGINE["EngineError"]


class AuthFailedError(Exception):
    """Stand-in for ConfigEntryAuthFailed."""


class UpdateFailedError(Exception):
    """Stand-in for the coordinator's UpdateFailed."""


@pytest.mark.parametrize(
    ("status", "state", "problem"),
    [
        ({"usb_state": "active", "storage_warning": 0}, "ok", False),
        ({"usb_state": "active", "storage_warning": 3}, "almost_full", False),
        ({"usb_state": "active", "usb_storage_full": True}, "full", True),
        ({"usb_state": "memory_full"}, "full", True),
        ({"usb_state": "format_required"}, "format_required", True),
        ({"usb_state": "unavailable"}, "removed", True),
        ({"usb_state": "unmounted"}, "unmounted", True),
        ({"usb_state": "incompatible"}, "incompatible", True),
        ({"usb_state": ""}, None, None),
        ({"usb_state": "future_state"}, None, None),
        (None, None, None),
        ({"usb_state": "active", "last_backup_result": "Failed"}, "ok", True),
        ({"usb_state": "", "last_backup_result": "error"}, None, True),
        ({"usb_state": "active", "last_backup_result": "success"}, "ok", False),
    ],
)
def test_usb_state_and_problem_mapping(status, state, problem) -> None:
    health = usb()
    assert health.usb_state(status) == state
    assert health.usb_problem(status) is problem
    assert state is None or state in health.USB_STATES
    assert "unavailable" not in health.USB_STATES, "reserved by Home Assistant"


def test_usb_attributes_expose_provider_facts_only() -> None:
    attributes = usb().usb_attributes(
        {
            "usb_state": "active",
            "usb_storage_used": 42,
            "storage_warning": 1,
            "last_backup_completed": "2026-10-04T01:00:00Z",
            "sync_module_id": "secret",
        }
    )
    assert attributes["percent_used"] == 42 and attributes["storage_warning"] == 1
    assert attributes["last_backup_completed"] == "2026-10-04T01:00:00Z"
    assert attributes["backup_failed"] is False
    assert "sync_module_id" not in attributes


def fake_response(status: int, body: object) -> SimpleNamespace:
    json_reader = (
        AsyncMock(side_effect=body) if isinstance(body, Exception) else AsyncMock(return_value=body)
    )
    return SimpleNamespace(status=status, json=json_reader, release=Mock())


@pytest.mark.parametrize(
    ("status", "body", "code", "reauth"),
    [
        (403, {"error": "reauth_required"}, "reauth_required", True),
        (403, ValueError("html"), None, False),
        (403, {"error": "Blink cloud request failed"}, "Blink cloud request failed", False),
        (401, {"error": "reauth_required"}, None, False),
    ],
)
async def test_client_reads_only_the_engine_error_code(status, body, code, reauth) -> None:
    client = object.__new__(ENGINE["EngineClient"])
    response = fake_response(status, body)
    client._session = SimpleNamespace(request=AsyncMock(return_value=response))
    client._headers, client._base_url = {}, "http://engine"
    with pytest.raises(EngineError) as raised:
        await client.get_json("/v1/state")
    assert (raised.value.status, raised.value.code) == (status, code)
    assert raised.value.reauth_required is reauth
    response.release.assert_called_once()
    if status == 401:
        response.json.assert_not_awaited()


def coordinator(error: Exception | None):
    issues = SimpleNamespace(
        async_create_issue=Mock(),
        async_delete_issue=Mock(),
        IssueSeverity=SimpleNamespace(ERROR="error"),
    )
    namespace = load(
        "runtime.py",
        {
            "ConfigEntryAuthFailed": AuthFailedError,
            "UpdateFailed": UpdateFailedError,
            "EngineError": EngineError,
            "ir": issues,
            "DOMAIN": "blink_live_bridge",
            "BRIDGE_TOKEN_ISSUE": "bridge_token_rejected",
        },
        {"BlinkCoordinator"},
    )
    instance = object.__new__(namespace["BlinkCoordinator"])
    instance.hass, instance._initial_update = object(), False
    post = AsyncMock(side_effect=error) if error else AsyncMock(return_value={})
    instance.client = SimpleNamespace(post=post, get_json=AsyncMock(return_value={"cameras": []}))
    return instance, issues


async def test_blink_revocation_starts_reauth_and_bridge_401_does_not() -> None:
    instance, issues = coordinator(EngineError("revoked", 403, "reauth_required"))
    with pytest.raises(AuthFailedError):
        await instance._async_update_data()
    issues.async_create_issue.assert_not_called()

    instance, issues = coordinator(EngineError("bridge", 401))
    with pytest.raises(UpdateFailedError):
        await instance._async_update_data()
    assert issues.async_create_issue.call_args.args[2] == "bridge_token_rejected"
    assert issues.async_create_issue.call_args.kwargs["is_fixable"] is False

    instance, issues = coordinator(EngineError("rate limited", 403))
    with pytest.raises(UpdateFailedError):
        await instance._async_update_data()
    issues.async_create_issue.assert_not_called()

    instance, issues = coordinator(None)
    assert await instance._async_update_data() == {"cameras": []}
    issues.async_delete_issue.assert_called_once()


async def test_storage_coordinator_maps_by_network_and_tolerates_old_engines() -> None:
    namespace = load(
        "storage_status.py",
        {
            "UpdateFailed": UpdateFailedError,
            "EngineError": EngineError,
            "STORAGE_TIMEOUT": 60,
            "MAX_STORAGES": 16,
        },
        {"BlinkStorageCoordinator"},
    )
    instance = object.__new__(namespace["BlinkStorageCoordinator"])
    instance.supported = True
    payload = {"storages": [{"network_id": "7", "status": {"usb_state": "active"}}, {}]}
    instance.client = SimpleNamespace(get_json=AsyncMock(return_value=payload))
    assert await instance._async_update_data() == {"7": {"usb_state": "active"}}
    instance.client.get_json = AsyncMock(return_value={"storages": "invalid"})
    with pytest.raises(UpdateFailedError):
        await instance._async_update_data()
    instance.client.get_json = AsyncMock(side_effect=EngineError("old", 404))
    assert await instance._async_update_data() == {}
    assert instance.supported is False
