"""Native Sync Module status refresh and reversible eject/mount commands."""

from typing import Any

import voluptuous as vol
from homeassistant.components import websocket_api
from homeassistant.core import HomeAssistant

from .client import EngineError
from .const import DOMAIN
from .runtime import BridgeRuntime

# The engine polls a Blink command for up to 90 s before answering.
STORAGE_COMMAND_TIMEOUT = 120
POSITIVE = vol.All(int, vol.Range(min=1))


def command_error_code(error: EngineError) -> str:
    """Map the engine outcome so the panel can say rejected versus still pending."""
    return {409: "rejected", 422: "invalid_state", 504: "pending"}.get(
        error.status or 0, "unavailable"
    )


async def refresh_storage_entities(runtime: BridgeRuntime) -> None:
    """Let the USB health entities follow a completed command without waiting."""
    if runtime.storage is not None:
        await runtime.storage.async_request_refresh()


@websocket_api.websocket_command({vol.Required("type"): "blink_live_bridge/local_storage/status"})
@websocket_api.async_response
async def ws_local_storage_status(
    hass: HomeAssistant,
    connection: websocket_api.ActiveConnection,
    msg: dict[str, Any],
) -> None:
    """Return status only; unlike the inventory it never rebuilds the USB manifest."""
    runtime = hass.data.get(DOMAIN, {}).get("runtime")
    if not isinstance(runtime, BridgeRuntime):
        connection.send_error(msg["id"], "unavailable", "Vistoda Blink is not loaded")
        return
    try:
        result = await runtime.client.get_json("/v1/local-storage/status")
    except EngineError:
        connection.send_error(msg["id"], "unavailable", "Blink USB status is unavailable")
        return
    storages = result.get("storages")
    if not isinstance(storages, list) or len(storages) > 16:
        connection.send_error(msg["id"], "invalid_response", "Blink USB status is invalid")
        return
    connection.send_result(msg["id"], {"storages": storages})


@websocket_api.websocket_command(
    {
        vol.Required("type"): "blink_live_bridge/local_storage/command",
        vol.Required("network_id"): POSITIVE,
        vol.Required("sync_module_id"): POSITIVE,
        vol.Required("command"): vol.In(("eject", "mount")),
    }
)
@websocket_api.async_response
async def ws_local_storage_command(
    hass: HomeAssistant,
    connection: websocket_api.ActiveConnection,
    msg: dict[str, Any],
) -> None:
    """Safely eject or reconnect one exact support, as the official app does."""
    if not connection.user.is_admin:
        connection.send_error(msg["id"], "unauthorized", "Administrator access required")
        return
    runtime = hass.data.get(DOMAIN, {}).get("runtime")
    if not isinstance(runtime, BridgeRuntime):
        connection.send_error(msg["id"], "unavailable", "Vistoda Blink is not loaded")
        return
    path = f"/v1/local-storage/{msg['network_id']}/{msg['sync_module_id']}/{msg['command']}"
    try:
        await runtime.client.post(path, None, STORAGE_COMMAND_TIMEOUT)
    except EngineError as error:
        connection.send_error(msg["id"], command_error_code(error), "Blink USB command failed")
        return
    connection.send_result(msg["id"], {})
    await refresh_storage_entities(runtime)
