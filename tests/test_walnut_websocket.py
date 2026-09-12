"""Execute actual relay methods without an HA install or provider connection."""

import ast
import asyncio
import base64
import binascii
import contextlib
from dataclasses import dataclass, field
from pathlib import Path
from types import ModuleType, SimpleNamespace
from unittest.mock import AsyncMock, Mock

import pytest
from aiohttp import ClientError, WSMsgType

ROOT = Path(__file__).parents[1] / "custom_components/blink_live_bridge"


@pytest.fixture
def relay(monkeypatch):
    module = ModuleType("walnut_test_runtime")
    monkeypatch.setitem(__import__("sys").modules, module.__name__, module)
    module.__dict__.update(
        {
            "asyncio": asyncio,
            "base64": base64,
            "binascii": binascii,
            "contextlib": contextlib,
            "dataclass": dataclass,
            "field": field,
            "ClientError": ClientError,
            "WSMsgType": WSMsgType,
            "DOMAIN": "blink_live_bridge",
            "EngineError": ClientError,
        }
    )
    for name in ("webrtc_websocket.py", "walnut_websocket.py"):
        parsed = ast.parse((ROOT / name).read_text())
        nodes = []
        for node in parsed.body:
            if isinstance(node, ast.ClassDef):
                nodes.append(node)
            elif isinstance(node, ast.FunctionDef | ast.AsyncFunctionDef):
                node.decorator_list = []
                nodes.append(node)
        tree = ast.Module(
            body=[
                ast.ImportFrom(module="__future__", names=[ast.alias(name="annotations")], level=0),
                *nodes,
            ],
            type_ignores=[],
        )
        exec(compile(ast.fix_missing_locations(tree), name, "exec"), module.__dict__)
    return module


def owned_session(relay):
    connection = SimpleNamespace(
        user=SimpleNamespace(is_admin=True),
        send_error=Mock(),
        send_result=Mock(),
        send_event=Mock(),
        subscriptions={11: Mock()},
    )
    hass = SimpleNamespace(data={})
    session = relay.WalnutSession(id(connection), "test", 11)
    session.upstream = SimpleNamespace(
        send_json=AsyncMock(), send_bytes=AsyncMock(), close=AsyncMock()
    )
    relay._sessions(hass)["handle"] = session
    return hass, connection, session


@pytest.mark.parametrize("intruder", ["owner", "admin"])
async def test_control_rejects_other_connection_or_nonadmin(relay, intruder):
    hass, connection, session = owned_session(relay)
    if intruder == "owner":
        session.owner = -1
    else:
        connection.user.is_admin = False
    await relay.ws_control(hass, connection, {"id": 1, "session_id": "handle", "action": "stop"})
    connection.send_error.assert_called_once()
    session.upstream.close.assert_not_awaited()
    assert not session.closed


async def test_exact_pcm_payload_and_epoch_request_reach_only_owned_upstream(relay):
    hass, connection, session = owned_session(relay)
    pcm = bytes(1024)
    await relay.ws_control(
        hass,
        connection,
        {"id": 1, "session_id": "handle", "action": "pcm", "data": base64.b64encode(pcm).decode()},
    )
    session.upstream.send_bytes.assert_awaited_once_with(pcm)
    await relay.ws_control(
        hass,
        connection,
        {
            "id": 2,
            "session_id": "handle",
            "action": "microphone",
            "enabled": True,
            "request_id": 72,
        },
    )
    session.upstream.send_json.assert_awaited_once_with(
        {"type": "microphone", "enabled": True, "request_id": 72}
    )


async def test_wrong_pcm_size_closes_owner_without_forwarding(relay):
    hass, connection, session = owned_session(relay)
    upstream = session.upstream
    await relay.ws_control(
        hass,
        connection,
        {
            "id": 1,
            "session_id": "handle",
            "action": "pcm",
            "data": base64.b64encode(bytes(1280)).decode(),
        },
    )
    upstream.send_bytes.assert_not_awaited()
    upstream.close.assert_awaited_once()
    assert session.closed


async def test_concurrent_writer_refused_without_queuing_audio(relay):
    hass, connection, session = owned_session(relay)
    async with session.writer:
        await relay.ws_control(
            hass,
            connection,
            {
                "id": 1,
                "session_id": "handle",
                "action": "pcm",
                "data": base64.b64encode(bytes(1024)).decode(),
            },
        )
    assert connection.send_error.call_args.args[1] == "busy"
    session.upstream.send_bytes.assert_not_awaited()


async def test_media_credit_is_no_longer_a_valid_microphone_control(relay):
    hass, connection, session = owned_session(relay)
    await relay.ws_control(
        hass,
        connection,
        {"id": 1, "session_id": "handle", "action": "ack", "sequence": 4},
    )
    assert connection.send_error.call_args.args[1] == "invalid_format"
    session.upstream.send_json.assert_not_awaited()
    assert not session.closed


async def test_stop_revokes_session_and_subscription_before_closing_upstream(relay):
    hass, connection, session = owned_session(relay)
    upstream = session.upstream
    await relay.ws_control(hass, connection, {"id": 1, "session_id": "handle", "action": "stop"})
    assert not relay._sessions(hass)
    assert not connection.subscriptions
    assert session.closed and session.upstream is None
    upstream.send_json.assert_awaited_once_with({"type": "stop"})
    upstream.close.assert_awaited_once()


class EventUpstream:
    def __init__(self, messages):
        self.messages = iter(messages)
        self.close = AsyncMock()

    def __aiter__(self):
        return self

    async def __anext__(self):
        try:
            return next(self.messages)
        except StopIteration:
            raise StopAsyncIteration from None


async def test_microphone_status_relay_needs_no_media_ack_and_keeps_epoch(relay):
    hass, connection, session = owned_session(relay)
    events = [
        {"type": "audio_offer", "supported": True, "stream_aec": False, "sent_frames": 16},
        {"type": "microphone", "enabled": True, "request_id": 72},
        {"type": "microphone", "enabled": False, "request_id": 72},
    ]
    session.upstream = EventUpstream(
        [SimpleNamespace(type=WSMsgType.TEXT, json=lambda event=event: event) for event in events]
    )
    upstream = session.upstream
    await asyncio.wait_for(relay._relay(hass, connection, "handle", session), 0.2)
    assert [call.args[1] for call in connection.send_event.call_args_list] == [
        *events,
        {"type": "closed"},
    ]
    assert not relay._sessions(hass)
    assert not connection.subscriptions
    upstream.close.assert_awaited_once()


async def test_unexpected_media_is_not_exposed_on_microphone_channel(relay):
    hass, connection, session = owned_session(relay)
    session.upstream = EventUpstream([SimpleNamespace(type=WSMsgType.BINARY, data=b"MP4")])
    await relay._relay(hass, connection, "handle", session)
    assert [call.args[1] for call in connection.send_event.call_args_list] == [{"type": "closed"}]
    assert session.closed


async def test_stop_does_not_close_another_session(relay):
    hass, connection, session = owned_session(relay)
    other = relay.WalnutSession(-1, "another", 99)
    relay._sessions(hass)["other"] = other
    await relay.ws_control(hass, connection, {"id": 1, "session_id": "handle", "action": "stop"})
    assert session.closed
    assert relay._sessions(hass) == {"other": other}
    assert not other.closed


async def test_subscribe_requires_admin_before_opening_provider_channel(relay, monkeypatch):
    hass, connection, _session = owned_session(relay)
    connection.user.is_admin = False
    runtime = Mock()
    monkeypatch.setattr(relay, "_runtime", runtime)
    await relay.ws_start(hass, connection, {"id": 2, "alias": "test"})
    assert connection.send_error.call_args.args[1] == "unauthorized"
    runtime.assert_not_called()


async def test_duplicate_alias_is_exclusive_before_opening_provider_channel(relay, monkeypatch):
    hass, connection, session = owned_session(relay)
    client = SimpleNamespace(websocket=AsyncMock())
    monkeypatch.setattr(relay, "_runtime", lambda _: SimpleNamespace(client=client))
    await relay.ws_start(hass, connection, {"id": 2, "alias": session.alias})
    assert connection.send_error.call_args.args[1] == "busy"
    client.websocket.assert_not_awaited()
    assert not session.closed
