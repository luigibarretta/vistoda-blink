"""Authenticated Vistoda Blink settings boundary for the unified panel."""

from typing import Any

import voluptuous as vol
from homeassistant.components import websocket_api
from homeassistant.core import HomeAssistant, callback

from .client import EngineError
from .const import DOMAIN
from .runtime import BridgeRuntime

ALIAS = vol.All(str, vol.Match(r"^[a-z0-9_-]{1,64}$"))
SETTING_KEY = vol.All(str, vol.Match(r"^[a-z_]{1,64}$"))
REVISION = vol.All(str, vol.Match(r"^[0-9a-f]{64}$"))


@callback
def async_register(hass: HomeAssistant) -> None:
    """Register redacted reads and admin-only typed writes once."""
    data = hass.data.setdefault(DOMAIN, {})
    if data.get("settings_websocket_registered"):
        return
    websocket_api.async_register_command(hass, ws_camera_settings)
    websocket_api.async_register_command(hass, ws_update_camera_setting)
    data["settings_websocket_registered"] = True


@websocket_api.websocket_command(
    {
        vol.Required("type"): "blink_live_bridge/camera/settings",
        vol.Required("alias"): ALIAS,
    }
)
@websocket_api.async_response
async def ws_camera_settings(
    hass: HomeAssistant,
    connection: websocket_api.ActiveConnection,
    msg: dict[str, Any],
) -> None:
    """Return only the provider's allowlisted camera configuration."""
    runtime = _runtime(hass)
    if runtime is None:
        connection.send_error(msg["id"], "unavailable", "Vistoda Blink is not loaded")
        return
    try:
        result = await runtime.client.get_json(f"/v1/cameras/{msg['alias']}/settings")
    except EngineError as error:
        _send_provider_error(connection, msg["id"], error)
        return
    connection.send_result(msg["id"], result)


@websocket_api.websocket_command(
    {
        vol.Required("type"): "blink_live_bridge/camera/settings/update",
        vol.Required("alias"): ALIAS,
        vol.Required("key"): SETTING_KEY,
        vol.Required("value"): vol.Any(bool, int, str),
        vol.Required("revision"): REVISION,
    }
)
@websocket_api.async_response
async def ws_update_camera_setting(
    hass: HomeAssistant,
    connection: websocket_api.ActiveConnection,
    msg: dict[str, Any],
) -> None:
    """Apply one typed setting for an administrator and refresh HA state."""
    if not connection.user.is_admin:
        connection.send_error(msg["id"], "unauthorized", "Administrator access required")
        return
    runtime = _runtime(hass)
    if runtime is None:
        connection.send_error(msg["id"], "unavailable", "Vistoda Blink is not loaded")
        return
    payload = {key: msg[key] for key in ("key", "value", "revision")}
    try:
        result = await runtime.client.post(
            f"/v1/cameras/{msg['alias']}/settings",
            payload,
        )
        state = await runtime.client.get_json("/v1/state")
        runtime.coordinator.async_set_updated_data(state)
    except EngineError as error:
        _send_provider_error(connection, msg["id"], error)
        return
    connection.send_result(msg["id"], result)


def _runtime(hass: HomeAssistant) -> BridgeRuntime | None:
    runtime = hass.data.get(DOMAIN, {}).get("runtime")
    return runtime if isinstance(runtime, BridgeRuntime) else None


def _send_provider_error(
    connection: websocket_api.ActiveConnection,
    message_id: int,
    error: EngineError,
) -> None:
    code = "conflict" if error.status == 409 else "unavailable"
    connection.send_error(message_id, code, "Blink camera setting is unavailable")
