"""Authenticated Vistoda Blink local recording boundary."""

from typing import Any

import voluptuous as vol
from homeassistant.components import websocket_api
from homeassistant.core import HomeAssistant, callback

from .client import EngineError
from .const import DOMAIN
from .runtime import BridgeRuntime

ALIAS = vol.All(str, vol.Match(r"^[a-z0-9_-]{1,64}$"))
RECORDING_ID = vol.All(str, vol.Match(r"^[0-9a-f-]{36}$"))
REQUEST_ID = vol.All(str, vol.Match(r"^[0-9a-f-]{36}$"))


@callback
def async_register(hass: HomeAssistant) -> None:
    """Register bounded local recording operations exactly once."""
    data = hass.data.setdefault(DOMAIN, {})
    if data.get("recording_websocket_registered"):
        return
    websocket_api.async_register_command(hass, ws_list_recordings)
    websocket_api.async_register_command(hass, ws_create_recording)
    websocket_api.async_register_command(hass, ws_delete_recording)
    data["recording_websocket_registered"] = True


@websocket_api.websocket_command(
    {
        vol.Required("type"): "blink_live_bridge/recordings/list",
        vol.Optional("alias"): ALIAS,
        vol.Optional("page", default=1): vol.All(int, vol.Range(min=1)),
        vol.Optional("page_size", default=10): vol.All(int, vol.Range(min=1, max=100)),
    }
)
@websocket_api.async_response
async def ws_list_recordings(
    hass: HomeAssistant,
    connection: websocket_api.ActiveConnection,
    msg: dict[str, Any],
) -> None:
    """List redacted local recording manifests."""
    runtime = _runtime(hass)
    if runtime is None:
        connection.send_error(msg["id"], "unavailable", "Vistoda Blink is not loaded")
        return
    try:
        camera = f"&camera={msg['alias']}" if msg.get("alias") else ""
        result = await runtime.client.get_json(
            f"/v1/recordings?page={msg['page']}&page_size={msg['page_size']}{camera}"
        )
    except EngineError:
        connection.send_error(msg["id"], "unavailable", "Blink recordings are unavailable")
        return
    connection.send_result(msg["id"], result)


@websocket_api.websocket_command(
    {
        vol.Required("type"): "blink_live_bridge/recordings/create",
        vol.Required("alias"): ALIAS,
        vol.Required("duration_seconds"): vol.In((15, 30, 60)),
        vol.Required("request_id"): REQUEST_ID,
    }
)
@websocket_api.async_response
async def ws_create_recording(
    hass: HomeAssistant,
    connection: websocket_api.ActiveConnection,
    msg: dict[str, Any],
) -> None:
    """Start one fixed-duration recording from the shared live stream."""
    if not connection.user.is_admin:
        connection.send_error(msg["id"], "unauthorized", "Administrator access required")
        return
    runtime = _runtime(hass)
    if runtime is None:
        connection.send_error(msg["id"], "unavailable", "Vistoda Blink is not loaded")
        return
    payload = {key: msg[key] for key in ("duration_seconds", "request_id")}
    try:
        result = await runtime.client.post(
            f"/v1/cameras/{msg['alias']}/recordings",
            payload,
        )
    except EngineError as error:
        code = "conflict" if error.status == 409 else "unavailable"
        connection.send_error(msg["id"], code, "Blink recording could not be started")
        return
    connection.send_result(msg["id"], result)


@websocket_api.websocket_command(
    {
        vol.Required("type"): "blink_live_bridge/recordings/delete",
        vol.Required("recording_id"): RECORDING_ID,
    }
)
@websocket_api.async_response
async def ws_delete_recording(
    hass: HomeAssistant,
    connection: websocket_api.ActiveConnection,
    msg: dict[str, Any],
) -> None:
    """Delete one completed local recording and its manifest."""
    if not connection.user.is_admin:
        connection.send_error(msg["id"], "unauthorized", "Administrator access required")
        return
    runtime = _runtime(hass)
    if runtime is None:
        connection.send_error(msg["id"], "unavailable", "Vistoda Blink is not loaded")
        return
    try:
        await runtime.client.delete(f"/v1/recordings/{msg['recording_id']}")
    except EngineError as error:
        code = "conflict" if error.status == 409 else "unavailable"
        connection.send_error(msg["id"], code, "Blink recording could not be deleted")
        return
    connection.send_result(msg["id"], {})


def _runtime(hass: HomeAssistant) -> BridgeRuntime | None:
    runtime = hass.data.get(DOMAIN, {}).get("runtime")
    return runtime if isinstance(runtime, BridgeRuntime) else None
