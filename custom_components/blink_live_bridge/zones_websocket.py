"""Authenticated Blink activity and privacy zone boundary."""

from typing import Any

import voluptuous as vol
from homeassistant.components import websocket_api
from homeassistant.core import HomeAssistant, callback

from .client import EngineError
from .const import DOMAIN
from .runtime import BridgeRuntime

ALIAS = vol.All(str, vol.Match(r"^[a-z0-9_-]{1,64}$"))
REVISION = vol.All(str, vol.Match(r"^[0-9a-f]{64}$"))
ACTIVITY_MASKS = vol.All(
    [vol.All(vol.Coerce(int), vol.Range(min=0, max=4095))],
    vol.Length(min=25, max=25),
)
PRIVACY_ZONE = vol.Schema(
    {
        vol.Required("x"): vol.All(vol.Coerce(int), vol.Range(min=0, max=19)),
        vol.Required("y"): vol.All(vol.Coerce(int), vol.Range(min=0, max=14)),
        vol.Required("w"): vol.All(vol.Coerce(int), vol.Range(min=1, max=20)),
        vol.Required("h"): vol.All(vol.Coerce(int), vol.Range(min=1, max=15)),
    },
    extra=vol.PREVENT_EXTRA,
)
PRIVACY_ZONES = vol.All([PRIVACY_ZONE], vol.Length(max=2))


@callback
def async_register(hass: HomeAssistant) -> None:
    """Register typed zone reads and admin-only writes once."""
    data = hass.data.setdefault(DOMAIN, {})
    if data.get("zones_websocket_registered"):
        return
    websocket_api.async_register_command(hass, ws_camera_zones)
    websocket_api.async_register_command(hass, ws_update_camera_zones)
    data["zones_websocket_registered"] = True


@websocket_api.websocket_command(
    {
        vol.Required("type"): "blink_live_bridge/camera/zones",
        vol.Required("alias"): ALIAS,
    }
)
@websocket_api.async_response
async def ws_camera_zones(
    hass: HomeAssistant,
    connection: websocket_api.ActiveConnection,
    msg: dict[str, Any],
) -> None:
    """Return only normalized zone geometry and activity masks."""
    runtime = _runtime(hass)
    if runtime is None:
        connection.send_error(msg["id"], "unavailable", "Vistoda Blink is not loaded")
        return
    try:
        result = await runtime.client.get_json(f"/v1/cameras/{msg['alias']}/zones")
    except EngineError as error:
        _send_error(connection, msg["id"], error)
        return
    connection.send_result(msg["id"], result)


@websocket_api.websocket_command(
    {
        vol.Required("type"): "blink_live_bridge/camera/zones/update",
        vol.Required("alias"): ALIAS,
        vol.Required("revision"): REVISION,
        vol.Required("activity_masks"): ACTIVITY_MASKS,
        vol.Required("privacy_zones"): PRIVACY_ZONES,
    }
)
@websocket_api.async_response
async def ws_update_camera_zones(
    hass: HomeAssistant,
    connection: websocket_api.ActiveConnection,
    msg: dict[str, Any],
) -> None:
    """Atomically apply and verify one complete normalized zone state."""
    if not connection.user.is_admin:
        connection.send_error(msg["id"], "unauthorized", "Administrator access required")
        return
    runtime = _runtime(hass)
    if runtime is None:
        connection.send_error(msg["id"], "unavailable", "Vistoda Blink is not loaded")
        return
    payload = {key: msg[key] for key in ("revision", "activity_masks", "privacy_zones")}
    try:
        result = await runtime.client.post(f"/v1/cameras/{msg['alias']}/zones", payload)
    except EngineError as error:
        _send_error(connection, msg["id"], error)
        return
    connection.send_result(msg["id"], result)


def _runtime(hass: HomeAssistant) -> BridgeRuntime | None:
    runtime = hass.data.get(DOMAIN, {}).get("runtime")
    return runtime if isinstance(runtime, BridgeRuntime) else None


def _send_error(
    connection: websocket_api.ActiveConnection, message_id: int, error: EngineError
) -> None:
    code = "conflict" if error.status == 409 else "unavailable"
    connection.send_error(message_id, code, "Blink camera zones are unavailable")
