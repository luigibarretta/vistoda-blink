"""Authenticated WebSocket boundary for Blink settings backups."""

from typing import Any

import voluptuous as vol
from homeassistant.components import websocket_api
from homeassistant.core import HomeAssistant, callback

from .client import EngineError
from .const import DOMAIN
from .runtime import BridgeRuntime
from .settings_backup import SettingsBackupManager

ALIAS = vol.All(str, vol.Match(r"^[a-z0-9_-]{1,64}$"))
BACKUP_ID = vol.All(str, vol.Match(r"^[0-9a-f]{32}$"))
ALIASES = vol.All([ALIAS], vol.Length(min=1, max=64))


@callback
def async_register(hass: HomeAssistant) -> None:
    """Register administrator-only backup commands once."""
    data = hass.data.setdefault(DOMAIN, {})
    if data.get("settings_backup_websocket_registered"):
        return
    data["settings_backup_manager"] = SettingsBackupManager(hass)
    for command in (ws_list, ws_create, ws_apply, ws_delete):
        websocket_api.async_register_command(hass, command)
    data["settings_backup_websocket_registered"] = True


def _manager(hass: HomeAssistant) -> SettingsBackupManager:
    return hass.data[DOMAIN]["settings_backup_manager"]


def _runtime(hass: HomeAssistant) -> BridgeRuntime | None:
    value = hass.data.get(DOMAIN, {}).get("runtime")
    return value if isinstance(value, BridgeRuntime) else None


def _admin(connection: websocket_api.ActiveConnection, message_id: int) -> bool:
    if connection.user.is_admin:
        return True
    connection.send_error(message_id, "unauthorized", "Administrator access required")
    return False


@websocket_api.websocket_command({vol.Required("type"): f"{DOMAIN}/camera/settings/backups"})
@websocket_api.async_response
async def ws_list(
    hass: HomeAssistant, connection: websocket_api.ActiveConnection, msg: dict[str, Any]
) -> None:
    if _admin(connection, msg["id"]):
        connection.send_result(msg["id"], {"backups": await _manager(hass).summaries()})


@websocket_api.websocket_command(
    {
        vol.Required("type"): f"{DOMAIN}/camera/settings/backups/create",
        vol.Optional("name", default="Blink settings"): vol.All(str, vol.Length(max=64)),
        vol.Optional("aliases"): ALIASES,
    }
)
@websocket_api.async_response
async def ws_create(
    hass: HomeAssistant, connection: websocket_api.ActiveConnection, msg: dict[str, Any]
) -> None:
    runtime = _runtime(hass)
    if not _admin(connection, msg["id"]):
        return
    if runtime is None:
        connection.send_error(msg["id"], "unavailable", "Vistoda Blink is not loaded")
        return
    try:
        result = await _manager(hass).create(runtime, msg["name"], msg.get("aliases"))
        connection.send_result(msg["id"], result)
    except EngineError:
        connection.send_error(msg["id"], "unavailable", "Blink settings backup failed")


@websocket_api.websocket_command(
    {
        vol.Required("type"): f"{DOMAIN}/camera/settings/backups/apply",
        vol.Required("backup_id"): BACKUP_ID,
        vol.Optional("aliases"): ALIASES,
    }
)
@websocket_api.async_response
async def ws_apply(
    hass: HomeAssistant, connection: websocket_api.ActiveConnection, msg: dict[str, Any]
) -> None:
    runtime = _runtime(hass)
    if not _admin(connection, msg["id"]):
        return
    if runtime is None:
        connection.send_error(msg["id"], "unavailable", "Vistoda Blink is not loaded")
        return
    try:
        result = await _manager(hass).apply(runtime, msg["backup_id"], msg.get("aliases"))
        connection.send_result(msg["id"], result)
    except LookupError:
        connection.send_error(msg["id"], "not_found", "Settings backup not found")
    except EngineError as error:
        code = (
            "conflict"
            if error.status == 409
            else "unsupported"
            if error.status == 422
            else "unavailable"
        )
        connection.send_error(msg["id"], code, "Blink settings restore failed")


@websocket_api.websocket_command(
    {
        vol.Required("type"): f"{DOMAIN}/camera/settings/backups/delete",
        vol.Required("backup_id"): BACKUP_ID,
    }
)
@websocket_api.async_response
async def ws_delete(
    hass: HomeAssistant, connection: websocket_api.ActiveConnection, msg: dict[str, Any]
) -> None:
    if not _admin(connection, msg["id"]):
        return
    deleted = await _manager(hass).delete(msg["backup_id"])
    connection.send_result(msg["id"], {"deleted": deleted})
