"""Owner-bound Walnut microphone relay; video remains on the separate HA stream."""

from __future__ import annotations

import asyncio
import base64
import binascii
import contextlib
from dataclasses import dataclass, field
from typing import Any
from uuid import uuid4

import voluptuous as vol
from aiohttp import ClientError, WSMsgType
from homeassistant.components import websocket_api
from homeassistant.core import HomeAssistant, callback

from .client import EngineError
from .const import DOMAIN
from .webrtc_websocket import ALIAS, HANDLE, WebRtcSession, _runtime


@dataclass(slots=True)
class WalnutSession(WebRtcSession):
    """A connection-owned microphone channel with bounded control concurrency."""

    writer: asyncio.Lock = field(default_factory=asyncio.Lock)


def _sessions(hass: HomeAssistant) -> dict[str, WalnutSession]:
    return hass.data.setdefault(DOMAIN, {}).setdefault("walnut_sessions", {})


@callback
def async_register(hass: HomeAssistant) -> None:
    data = hass.data.setdefault(DOMAIN, {})
    if not data.get("walnut_registered"):
        websocket_api.async_register_command(hass, ws_start)
        websocket_api.async_register_command(hass, ws_control)
        data["walnut_registered"] = True


@websocket_api.websocket_command(
    {
        vol.Required("type"): "blink_live_bridge/walnut/subscribe",
        vol.Required("alias"): ALIAS,
    }
)
@websocket_api.async_response
async def ws_start(
    hass: HomeAssistant, connection: websocket_api.ActiveConnection, msg: dict[str, Any]
) -> None:
    if not connection.user.is_admin:
        connection.send_error(msg["id"], "unauthorized", "Administrator access required")
        return
    runtime = _runtime(hass)
    sessions = _sessions(hass)
    if runtime is None or len(sessions) >= 4:
        connection.send_error(msg["id"], "unavailable", "Live Blink non disponibile")
        return
    # Viewing is shared; the engine grants the exclusive talk lease on mic enable.
    # Still reject duplicate subscriptions from the same HA connection.
    if any(
        item.alias == msg["alias"] and item.owner == id(connection) for item in sessions.values()
    ):
        connection.send_error(msg["id"], "busy", "La telecamera è già in uso")
        return
    handle = uuid4().hex
    session = WalnutSession(id(connection), msg["alias"], msg["id"])
    sessions[handle] = session

    @callback
    def unsubscribe() -> None:
        if sessions.pop(handle, None) is session:
            hass.async_create_task(session.stop())

    connection.subscriptions[msg["id"]] = unsubscribe
    try:
        async with asyncio.timeout(12):
            upstream = await runtime.client.websocket(f"/v1/cameras/{msg['alias']}/walnut")
        if session.closed or sessions.get(handle) is not session:
            async with asyncio.timeout(3):
                await upstream.close()
            return
        session.upstream = upstream
    except (ClientError, EngineError, TimeoutError):
        sessions.pop(handle, None)
        connection.subscriptions.pop(msg["id"], None)
        await session.stop(False)
        connection.send_error(msg["id"], "unavailable", "Live audio Blink non disponibile")
        return
    except asyncio.CancelledError:
        sessions.pop(handle, None)
        connection.subscriptions.pop(msg["id"], None)
        await asyncio.shield(session.stop(False))
        raise
    connection.send_result(msg["id"])
    connection.send_event(msg["id"], {"type": "ready", "session_id": handle})
    session.relay_task = hass.async_create_task(_relay(hass, connection, handle, session))


@websocket_api.websocket_command(
    {
        vol.Required("type"): "blink_live_bridge/walnut/control",
        vol.Required("session_id"): HANDLE,
        vol.Required("action"): vol.In(("microphone", "pcm", "ping", "stop")),
        vol.Optional("enabled"): bool,
        vol.Optional("request_id"): vol.All(int, vol.Range(min=0, max=4294967295)),
        vol.Optional("data"): vol.All(str, vol.Length(min=1368, max=1368)),
    }
)
@websocket_api.async_response
async def ws_control(
    hass: HomeAssistant, connection: websocket_api.ActiveConnection, msg: dict[str, Any]
) -> None:
    session = _sessions(hass).get(msg["session_id"])
    if not connection.user.is_admin or session is None or session.owner != id(connection):
        connection.send_error(msg["id"], "not_found", "Sessione Blink non trovata")
        return
    if session.closed or session.upstream is None:
        connection.send_error(msg["id"], "not_ready", "Sessione Blink chiusa")
        return
    action = msg["action"]
    if action not in {"microphone", "pcm", "ping", "stop"}:
        connection.send_error(msg["id"], "invalid_format", "Controllo microfono non valido")
        return
    if action == "stop":
        _sessions(hass).pop(msg["session_id"], None)
        connection.subscriptions.pop(session.subscription, None)
        await session.stop()
    elif session.writer.locked():
        connection.send_error(msg["id"], "busy", "Controllo audio in corso")
        return
    else:
        try:
            async with session.writer, asyncio.timeout(1):
                if action == "pcm":
                    pcm = base64.b64decode(msg.get("data", ""), validate=True)
                    if len(pcm) != 1024:
                        raise ValueError("Invalid PCM size")
                    await session.upstream.send_bytes(pcm)
                else:
                    payload = {"type": action}
                    if action == "microphone":
                        if "enabled" not in msg or "request_id" not in msg:
                            raise ValueError("Missing microphone state")
                        payload["enabled"] = msg["enabled"]
                        payload["request_id"] = msg["request_id"]
                    await session.upstream.send_json(payload)
        except (ClientError, ConnectionError, TimeoutError, ValueError, binascii.Error):
            await session.stop(False)
            connection.send_error(msg["id"], "unavailable", "Canale audio terminato")
            return
    connection.send_result(msg["id"])


async def _relay(
    hass: HomeAssistant,
    connection: websocket_api.ActiveConnection,
    handle: str,
    session: WalnutSession,
) -> None:
    upstream = session.upstream
    if upstream is None:
        return
    try:
        async for message in upstream:
            if message.type is WSMsgType.BINARY:
                # This route is control-only; never expose an unexpected media stream.
                break
            elif message.type is WSMsgType.TEXT:
                event = message.json()
                if isinstance(event, dict) and event.get("type") in {
                    "audio_offer",
                    "microphone",
                    "error",
                }:
                    connection.send_event(session.subscription, event)
            elif message.type in {WSMsgType.CLOSE, WSMsgType.CLOSED, WSMsgType.ERROR}:
                break
    except (ValueError, TypeError, ClientError, TimeoutError):
        pass
    finally:
        if _sessions(hass).pop(handle, None) is session:
            with contextlib.suppress(Exception):
                connection.send_event(session.subscription, {"type": "closed"})
        connection.subscriptions.pop(session.subscription, None)
        await session.stop(False)


async def async_stop_all(hass: HomeAssistant) -> None:
    sessions = list(_sessions(hass).values())
    _sessions(hass).clear()
    await asyncio.gather(*(session.stop() for session in sessions), return_exceptions=True)
