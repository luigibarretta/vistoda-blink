"""Authenticated and guarded Blink Sync Module storage boundary."""

from typing import Any

import voluptuous as vol
from homeassistant.components import websocket_api
from homeassistant.core import HomeAssistant, callback

from .client import EngineError
from .const import DOMAIN
from .runtime import BridgeRuntime


@callback
def async_register(hass: HomeAssistant) -> None:
    """Register bounded inventory and administrator storage commands once."""
    data = hass.data.setdefault(DOMAIN, {})
    if data.get("storage_websocket_registered"):
        return
    websocket_api.async_register_command(hass, ws_local_storage)
    websocket_api.async_register_command(hass, ws_delete_local_storage_clip)
    websocket_api.async_register_command(hass, ws_format_local_storage)
    data["storage_websocket_registered"] = True


@websocket_api.websocket_command(
    {
        vol.Required("type"): "blink_live_bridge/local_storage/list",
        vol.Optional("page", default=1): vol.All(int, vol.Range(min=1)),
        vol.Optional("page_size", default=10): vol.All(int, vol.Range(min=1, max=50)),
    }
)
@websocket_api.async_response
async def ws_local_storage(
    hass: HomeAssistant,
    connection: websocket_api.ActiveConnection,
    msg: dict[str, Any],
) -> None:
    """Return the bounded provider-owned USB inventory."""
    runtime = hass.data.get(DOMAIN, {}).get("runtime")
    if not isinstance(runtime, BridgeRuntime):
        connection.send_error(msg["id"], "unavailable", "Vistoda Blink is not loaded")
        return
    try:
        result = await runtime.client.get_json(
            f"/v1/local-storage?page={msg['page']}&page_size={msg['page_size']}"
        )
    except EngineError:
        connection.send_error(msg["id"], "unavailable", "Blink USB storage is unavailable")
        return
    storages = result.get("storages")
    if not isinstance(storages, list) or len(storages) > 16:
        connection.send_error(msg["id"], "invalid_response", "Blink USB inventory is invalid")
        return
    if any(not _valid_page(storage.get("pagination")) for storage in storages):
        connection.send_error(msg["id"], "invalid_response", "Blink USB page is invalid")
        return
    connection.send_result(msg["id"], {"storages": storages})


def _valid_page(value: object) -> bool:
    """Accept only the bounded provider pagination contract."""
    if not isinstance(value, dict):
        return False
    integers = ("page", "page_size", "total_items", "total_pages")
    return (
        all(isinstance(value.get(key), int) for key in integers)
        and 1 <= value["page_size"] <= 50
        and value["page"] >= 1
        and value["total_items"] >= 0
        and value["total_pages"] >= 1
        and isinstance(value.get("has_previous"), bool)
        and isinstance(value.get("has_next"), bool)
    )


POSITIVE = vol.All(int, vol.Range(min=1))


@websocket_api.websocket_command(
    {
        vol.Required("type"): "blink_live_bridge/local_storage/delete",
        vol.Required("network_id"): POSITIVE,
        vol.Required("sync_module_id"): POSITIVE,
        vol.Required("manifest_id"): POSITIVE,
        vol.Required("clip_id"): POSITIVE,
    }
)
@websocket_api.async_response
async def ws_delete_local_storage_clip(
    hass: HomeAssistant,
    connection: websocket_api.ActiveConnection,
    msg: dict[str, Any],
) -> None:
    """Delete one exact provider clip after the engine revalidates the manifest."""
    if not connection.user.is_admin:
        connection.send_error(msg["id"], "unauthorized", "Administrator access required")
        return
    runtime = hass.data.get(DOMAIN, {}).get("runtime")
    if not isinstance(runtime, BridgeRuntime):
        connection.send_error(msg["id"], "unavailable", "Vistoda Blink is not loaded")
        return
    path = (
        f"/v1/local-storage/{msg['network_id']}/{msg['sync_module_id']}/"
        f"{msg['manifest_id']}/{msg['clip_id']}"
    )
    try:
        await runtime.client.delete(path)
    except EngineError as error:
        code = "conflict" if error.status in {409, 422} else "unavailable"
        connection.send_error(msg["id"], code, "Blink USB clip could not be deleted")
        return
    connection.send_result(msg["id"], {})


@websocket_api.websocket_command(
    {
        vol.Required("type"): "blink_live_bridge/local_storage/format",
        vol.Required("network_id"): POSITIVE,
        vol.Required("sync_module_id"): POSITIVE,
        vol.Required("confirmation"): vol.All(str, vol.Length(min=10, max=80)),
    }
)
@websocket_api.async_response
async def ws_format_local_storage(
    hass: HomeAssistant,
    connection: websocket_api.ActiveConnection,
    msg: dict[str, Any],
) -> None:
    """Format one exact support only after an explicit typed confirmation."""
    if not connection.user.is_admin:
        connection.send_error(msg["id"], "unauthorized", "Administrator access required")
        return
    expected = f"FORMATTA {msg['network_id']}/{msg['sync_module_id']}"
    if msg["confirmation"] != expected:
        connection.send_error(msg["id"], "invalid_confirmation", "Confirmation does not match")
        return
    runtime = hass.data.get(DOMAIN, {}).get("runtime")
    if not isinstance(runtime, BridgeRuntime):
        connection.send_error(msg["id"], "unavailable", "Vistoda Blink is not loaded")
        return
    try:
        await runtime.client.post(
            f"/v1/local-storage/{msg['network_id']}/{msg['sync_module_id']}/format",
            {"confirmation": expected},
        )
    except EngineError:
        connection.send_error(msg["id"], "unavailable", "Blink USB could not be formatted")
        return
    connection.send_result(msg["id"], {})
