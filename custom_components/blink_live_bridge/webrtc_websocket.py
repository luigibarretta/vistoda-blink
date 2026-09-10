"""Owner-bound Blink WebRTC signaling over Home Assistant's authenticated socket."""

from __future__ import annotations

import asyncio
import contextlib
from dataclasses import dataclass
from typing import Any
from uuid import uuid4

import voluptuous as vol
from aiohttp import ClientError, ClientWebSocketResponse, WSMsgType
from homeassistant.components import websocket_api
from homeassistant.core import HomeAssistant, callback

from .client import EngineError
from .const import DOMAIN
from .runtime import BridgeRuntime

ALIAS = vol.All(str, vol.Match(r"^[a-z0-9_-]{1,64}$"))
HANDLE = vol.All(str, vol.Match(r"^[0-9a-f]{32}$"))
SDP = vol.All(str, vol.Length(min=5, max=96 * 1024))
CANDIDATE = vol.All(str, vol.Length(min=1, max=4096))
ACTION = vol.In(("activate", "ice", "microphone", "sdp", "speaker", "stop"))


@dataclass(slots=True)
class WebRtcSession:
    """One browser-owned local engine socket."""

    owner: int
    alias: str
    subscription: int
    upstream: ClientWebSocketResponse | None = None
    relay_task: asyncio.Task[Any] | None = None
    closed: bool = False

    async def stop(self, notify_upstream: bool = True) -> None:
        """Synchronously revoke ownership, then bound remote cleanup."""
        if self.closed:
            return
        self.closed = True
        upstream, self.upstream = self.upstream, None
        relay_task, self.relay_task = self.relay_task, None
        if relay_task is not None and relay_task is not asyncio.current_task():
            relay_task.cancel()
        if upstream is None:
            return
        if notify_upstream:
            with contextlib.suppress(Exception):
                async with asyncio.timeout(5):
                    await upstream.send_json({"type": "stop"})
        with contextlib.suppress(Exception):
            async with asyncio.timeout(5):
                await upstream.close()


@callback
def async_register(hass: HomeAssistant) -> None:
    """Register the signaling subscription and typed controls exactly once."""
    data = hass.data.setdefault(DOMAIN, {})
    if data.get("webrtc_websocket_registered"):
        return
    websocket_api.async_register_command(hass, ws_start)
    websocket_api.async_register_command(hass, ws_control)
    data["webrtc_sessions"] = {}
    data["webrtc_websocket_registered"] = True


@websocket_api.websocket_command(
    {
        vol.Required("type"): "blink_live_bridge/webrtc/subscribe",
        vol.Required("alias"): ALIAS,
        vol.Required("offer_sdp"): SDP,
    }
)
@websocket_api.async_response
async def ws_start(
    hass: HomeAssistant,
    connection: websocket_api.ActiveConnection,
    msg: dict[str, Any],
) -> None:
    """Start a session that belongs to this HA user connection."""
    if not connection.user.is_admin:
        connection.send_error(msg["id"], "unauthorized", "Administrator access required")
        return
    runtime = _runtime(hass)
    if runtime is None:
        connection.send_error(msg["id"], "unavailable", "Vistoda Blink is not loaded")
        return
    sessions = _sessions(hass)
    if any(item.alias == msg["alias"] for item in sessions.values()):
        connection.send_error(msg["id"], "busy", "La telecamera è già in uso")
        return
    handle = uuid4().hex
    session = WebRtcSession(id(connection), msg["alias"], msg["id"])
    sessions[handle] = session

    @callback
    def unsubscribe() -> None:
        current = sessions.pop(handle, None)
        if current is session:
            hass.async_create_task(session.stop())

    connection.subscriptions[msg["id"]] = unsubscribe
    try:
        async with asyncio.timeout(12):
            upstream = await runtime.client.websocket(f"/v1/cameras/{msg['alias']}/webrtc")
        if session.closed or sessions.get(handle) is not session:
            with contextlib.suppress(Exception):
                async with asyncio.timeout(5):
                    await upstream.close()
            return
        session.upstream = upstream
        async with asyncio.timeout(5):
            await upstream.send_json({"type": "start", "sdp": msg["offer_sdp"]})
    except asyncio.CancelledError:
        sessions.pop(handle, None)
        connection.subscriptions.pop(msg["id"], None)
        await asyncio.shield(session.stop(False))
        raise
    except (ClientError, EngineError, TimeoutError) as error:
        owned = sessions.pop(handle, None) is session
        connection.subscriptions.pop(msg["id"], None)
        await session.stop(False)
        if owned:
            with contextlib.suppress(Exception):
                legacy = isinstance(error, EngineError) and error.status == 412
                connection.send_error(
                    msg["id"],
                    "legacy_required" if legacy else "unavailable",
                    "Live Blink compatibile richiesto"
                    if legacy
                    else "Segnalazione Blink non disponibile",
                )
        return
    if session.closed or sessions.get(handle) is not session:
        await session.stop(False)
        return
    session.relay_task = hass.async_create_task(_relay(hass, connection, handle, session))
    connection.send_result(msg["id"], {"session_id": handle})
    connection.send_event(msg["id"], {"type": "ready", "session_id": handle})


@websocket_api.websocket_command(
    {
        vol.Required("type"): "blink_live_bridge/webrtc/control",
        vol.Required("session_id"): HANDLE,
        vol.Required("action"): ACTION,
        vol.Optional("enabled"): bool,
        vol.Optional("candidate"): CANDIDATE,
        vol.Optional("sdp_mid"): vol.Any(None, vol.All(str, vol.Length(max=64))),
        vol.Optional("sdp_mline_index"): vol.All(int, vol.Range(min=0, max=32)),
        vol.Optional("offer_sdp"): SDP,
        vol.Optional("reason"): vol.All(str, vol.Length(max=64)),
    }
)
@websocket_api.async_response
async def ws_control(
    hass: HomeAssistant,
    connection: websocket_api.ActiveConnection,
    msg: dict[str, Any],
) -> None:
    """Forward only allowlisted controls to the connection-owned session."""
    session = _sessions(hass).get(msg["session_id"])
    if session is None or session.owner != id(connection):
        connection.send_error(msg["id"], "not_found", "Sessione Blink non trovata")
        return
    if session.closed or session.upstream is None:
        connection.send_error(msg["id"], "not_ready", "Sessione Blink non pronta")
        return
    payload: dict[str, Any] = {"type": msg["action"]}
    if msg["action"] in {"microphone", "speaker"}:
        if "enabled" not in msg:
            connection.send_error(msg["id"], "invalid_format", "Stato mancante")
            return
        payload["enabled"] = msg["enabled"]
    elif msg["action"] == "ice":
        if "candidate" not in msg or "sdp_mline_index" not in msg:
            connection.send_error(msg["id"], "invalid_format", "Candidato ICE incompleto")
            return
        payload.update({key: msg.get(key) for key in ("candidate", "sdp_mid", "sdp_mline_index")})
    elif msg["action"] == "sdp":
        if "offer_sdp" not in msg:
            connection.send_error(msg["id"], "invalid_format", "Offerta SDP mancante")
            return
        payload.update({"sdp": msg["offer_sdp"], "reason": msg.get("reason")})
    if msg["action"] == "stop":
        _sessions(hass).pop(msg["session_id"], None)
        connection.subscriptions.pop(session.subscription, None)
        await session.stop()
    else:
        try:
            async with asyncio.timeout(5):
                await session.upstream.send_json(payload)
        except (ClientError, ConnectionError, TimeoutError):
            connection.send_error(msg["id"], "unavailable", "Sessione Blink terminata")
            return
    connection.send_result(msg["id"])


async def _relay(
    hass: HomeAssistant,
    connection: websocket_api.ActiveConnection,
    handle: str,
    session: WebRtcSession,
) -> None:
    upstream = session.upstream
    if upstream is None:
        return
    try:
        async for message in upstream:
            if message.type is WSMsgType.TEXT:
                event = message.json()
                if isinstance(event, dict):
                    connection.send_event(session.subscription, event)
            elif message.type in {WSMsgType.CLOSE, WSMsgType.CLOSED, WSMsgType.ERROR}:
                break
    except (ValueError, TypeError):
        connection.send_event(
            session.subscription, {"type": "error", "message": "Risposta Blink non valida"}
        )
    finally:
        if _sessions(hass).pop(handle, None) is session:
            with contextlib.suppress(Exception):
                connection.send_event(
                    session.subscription,
                    {"type": "closed", "message": "Sessione Blink terminata"},
                )
        connection.subscriptions.pop(session.subscription, None)
        await session.stop(False)


def _runtime(hass: HomeAssistant) -> BridgeRuntime | None:
    runtime = hass.data.get(DOMAIN, {}).get("runtime")
    return runtime if isinstance(runtime, BridgeRuntime) else None


def _sessions(hass: HomeAssistant) -> dict[str, WebRtcSession]:
    return hass.data.setdefault(DOMAIN, {}).setdefault("webrtc_sessions", {})


async def async_stop_all(hass: HomeAssistant) -> None:
    """Release every provider lease before the integration unloads."""
    sessions = list(_sessions(hass).values())
    _sessions(hass).clear()
    await asyncio.gather(*(session.stop() for session in sessions), return_exceptions=True)
