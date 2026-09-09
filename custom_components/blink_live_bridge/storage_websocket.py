"""Authenticated read-only Blink Sync Module storage boundary."""

from typing import Any

import voluptuous as vol
from homeassistant.components import websocket_api
from homeassistant.core import HomeAssistant, callback

from .client import EngineError
from .const import DOMAIN
from .runtime import BridgeRuntime


@callback
def async_register(hass: HomeAssistant) -> None:
    """Register the read-only inventory command once."""
    data = hass.data.setdefault(DOMAIN, {})
    if data.get("storage_websocket_registered"):
        return
    websocket_api.async_register_command(hass, ws_local_storage)
    data["storage_websocket_registered"] = True


@websocket_api.websocket_command({vol.Required("type"): "blink_live_bridge/local_storage/list"})
@websocket_api.async_response
async def ws_local_storage(
    hass: HomeAssistant,
    connection: websocket_api.ActiveConnection,
    msg: dict[str, Any],
) -> None:
    """Return the bounded provider-owned USB inventory without mutation controls."""
    runtime = hass.data.get(DOMAIN, {}).get("runtime")
    if not isinstance(runtime, BridgeRuntime):
        connection.send_error(msg["id"], "unavailable", "Vistoda Blink is not loaded")
        return
    try:
        result = await runtime.client.get_json("/v1/local-storage")
    except EngineError:
        connection.send_error(msg["id"], "unavailable", "Blink USB storage is unavailable")
        return
    storages = result.get("storages")
    if not isinstance(storages, list) or len(storages) > 16:
        connection.send_error(msg["id"], "invalid_response", "Blink USB inventory is invalid")
        return
    connection.send_result(msg["id"], {"storages": storages})
