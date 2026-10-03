"""Motion-recording settings through the authenticated HA WebSocket boundary."""

from typing import Any

import voluptuous as vol
from homeassistant.components import websocket_api
from homeassistant.core import HomeAssistant, callback

from .client import EngineError
from .const import DOMAIN
from .runtime import BridgeRuntime

ALIAS = vol.All(str, vol.Length(min=1, max=64), vol.Match(r"^[a-z0-9_-]+$"))


@callback
def async_register(hass: HomeAssistant) -> None:
    """Register the motion-recording commands once per Home Assistant instance."""
    data = hass.data.setdefault(DOMAIN, {})
    if data.get("motion_websocket_registered"):
        return
    websocket_api.async_register_command(hass, ws_motion_recording_get)
    websocket_api.async_register_command(hass, ws_motion_recording_set)
    data["motion_websocket_registered"] = True


def _runtime(hass: HomeAssistant) -> BridgeRuntime | None:
    runtime = hass.data.get(DOMAIN, {}).get("runtime")
    return runtime if isinstance(runtime, BridgeRuntime) else None


def _valid(result: object) -> bool:
    return (
        isinstance(result, dict)
        and isinstance(result.get("settings"), dict)
        and isinstance(result.get("cameras"), list)
        and len(result["cameras"]) <= 32
    )


@websocket_api.websocket_command({vol.Required("type"): "blink_live_bridge/motion_recording/get"})
@websocket_api.async_response
async def ws_motion_recording_get(
    hass: HomeAssistant, connection: websocket_api.ActiveConnection, msg: dict[str, Any]
) -> None:
    """Return settings, selectable cameras and the poller state."""
    runtime = _runtime(hass)
    if runtime is None:
        connection.send_error(msg["id"], "unavailable", "Vistoda Blink is not loaded")
        return
    try:
        result = await runtime.client.get_json("/v1/motion/recording")
    except EngineError:
        connection.send_error(msg["id"], "unavailable", "Motion recording is unavailable")
        return
    if not _valid(result):
        connection.send_error(msg["id"], "invalid_response", "Motion settings are invalid")
        return
    connection.send_result(msg["id"], result)


@websocket_api.websocket_command(
    {
        vol.Required("type"): "blink_live_bridge/motion_recording/set",
        vol.Required("enabled"): bool,
        vol.Required("duration_seconds"): vol.In((15, 30, 60)),
        vol.Required("cameras"): vol.All([ALIAS], vol.Length(max=32)),
    }
)
@websocket_api.async_response
async def ws_motion_recording_set(
    hass: HomeAssistant, connection: websocket_api.ActiveConnection, msg: dict[str, Any]
) -> None:
    """Persist motion-recording settings in the provider (administrators only)."""
    if not connection.user.is_admin:
        connection.send_error(msg["id"], "unauthorized", "Administrator access required")
        return
    runtime = _runtime(hass)
    if runtime is None:
        connection.send_error(msg["id"], "unavailable", "Vistoda Blink is not loaded")
        return
    payload = {
        "enabled": msg["enabled"],
        "duration_seconds": msg["duration_seconds"],
        "cameras": list(dict.fromkeys(msg["cameras"])),
    }
    try:
        result = await runtime.client.put_json("/v1/motion/recording", payload)
    except EngineError as error:
        code = "invalid" if error.status in {404, 422, 400} else "unavailable"
        connection.send_error(msg["id"], code, "Motion settings were not saved")
        return
    if not _valid(result):
        connection.send_error(msg["id"], "invalid_response", "Motion settings are invalid")
        return
    connection.send_result(msg["id"], result)
