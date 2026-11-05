"""Standalone Vistoda Blink setup with stable Home Assistant contracts."""

import voluptuous as vol
from homeassistant import config_entries
from homeassistant.config_entries import ConfigEntry
from homeassistant.core import HomeAssistant
from homeassistant.exceptions import ConfigEntryNotReady
from homeassistant.helpers import device_registry as dr
from homeassistant.helpers.typing import ConfigType

from .client import EngineClient, EngineError
from .const import (
    CONF_TOKEN,
    CONF_URL,
    DOMAIN,
    ENGINE_URL,
    LEGACY_PARENT_IDENTIFIER,
    PARENT_IDENTIFIER,
    PLATFORMS,
)
from .http import register_views
from .motion import BlinkMotionCoordinator
from .motion_websocket import async_register as async_register_motion_websocket
from .recording_websocket import async_register as async_register_recording_websocket
from .runtime import BlinkCoordinator, BridgeRuntime, scan_interval
from .services import async_setup_services
from .settings_backup_websocket import async_register as async_register_settings_backup
from .storage_status import BlinkStorageCoordinator
from .storage_websocket import async_register as async_register_storage_websocket
from .walnut_websocket import async_register as async_register_walnut_websocket
from .walnut_websocket import async_stop_all as async_stop_walnut_sessions
from .webrtc_websocket import (
    async_register as async_register_webrtc_websocket,
)
from .webrtc_websocket import (
    async_stop_all as async_stop_webrtc_sessions,
)
from .websocket import async_register as async_register_websocket
from .zones_websocket import async_register as async_register_zones_websocket

CONFIG_SCHEMA = vol.Schema(
    {vol.Optional(DOMAIN): vol.Schema({vol.Required(CONF_TOKEN): vol.Match(r"^[0-9a-f]{64}$")})},
    extra=vol.ALLOW_EXTRA,
)


async def async_setup(hass: HomeAssistant, config: ConfigType) -> bool:
    """Store only the local workload token."""
    async_setup_services(hass)
    async_register_websocket(hass)
    async_register_webrtc_websocket(hass)
    async_register_walnut_websocket(hass)
    async_register_zones_websocket(hass)
    async_register_recording_websocket(hass)
    async_register_motion_websocket(hass)
    async_register_storage_websocket(hass)
    async_register_settings_backup(hass)
    data = hass.data.setdefault(DOMAIN, {})
    if DOMAIN in config:
        data[CONF_TOKEN] = config[DOMAIN][CONF_TOKEN]
    return True


async def async_setup_entry(hass: HomeAssistant, entry: ConfigEntry) -> bool:
    """Load the Rust provider and all official-parity platforms."""
    data = hass.data.setdefault(DOMAIN, {})
    token = entry.data.get(CONF_TOKEN) or data.get(CONF_TOKEN)
    if not token:
        raise ConfigEntryNotReady("Vistoda Blink workload credential is unavailable")
    client = EngineClient(hass, token, entry.data.get(CONF_URL, ENGINE_URL))
    status = await _status_or_retry(client)
    if not status.get("enrolled"):
        raise ConfigEntryNotReady("Vistoda Blink requires standalone enrollment")
    coordinator = BlinkCoordinator(hass, client, scan_interval(entry.options))
    await coordinator.async_config_entry_first_refresh()
    motion = BlinkMotionCoordinator(hass, client)
    await motion.async_refresh()
    # Cancelled with the entry; it ends at once on providers without long-poll.
    entry.async_create_background_task(hass, motion.async_long_poll(), f"{DOMAIN}_motion_long_poll")
    # USB health polls only while a Sync Module exists (entities subscribe).
    storage = None
    if any(network.get("has_sync_module") for network in coordinator.data.get("networks", [])):
        storage = BlinkStorageCoordinator(hass, client)
        await storage.async_refresh()
    parent_device = _parent_device(hass, entry)
    data[entry.entry_id] = BridgeRuntime(
        client=client,
        coordinator=coordinator,
        token=token,
        parent_device_id=parent_device.id,
        motion=motion,
        storage=storage,
    )
    data["runtime"] = data[entry.entry_id]
    if not data.get("views_registered"):
        register_views(hass)
        data["views_registered"] = True
    await hass.config_entries.async_forward_entry_setups(entry, PLATFORMS)
    if not data.get("vistoda_discovery_started"):
        data["vistoda_discovery_started"] = True
        hass.async_create_task(_discover_vistoda(hass))
    return True


async def async_unload_entry(hass: HomeAssistant, entry: ConfigEntry) -> bool:
    """Unload entities without touching the standalone provider session."""
    if not await hass.config_entries.async_unload_platforms(entry, PLATFORMS):
        return False
    await async_stop_webrtc_sessions(hass)
    await async_stop_walnut_sessions(hass)
    hass.data[DOMAIN].pop(entry.entry_id, None)
    hass.data[DOMAIN].pop("runtime", None)
    return True


def _parent_device(hass: HomeAssistant, entry: ConfigEntry) -> dr.DeviceEntry:
    """Create the adapter device under this domain, migrating our legacy one."""
    registry = dr.async_get(hass)
    legacy = registry.async_get_device(identifiers={LEGACY_PARENT_IDENTIFIER})
    if (
        legacy
        and legacy.config_entries == {entry.entry_id}
        and not registry.async_get_device(identifiers={PARENT_IDENTIFIER})
    ):
        registry.async_update_device(
            legacy.id,
            new_identifiers=(legacy.identifiers - {LEGACY_PARENT_IDENTIFIER}) | {PARENT_IDENTIFIER},
        )
    return registry.async_get_or_create(
        config_entry_id=entry.entry_id,
        identifiers={PARENT_IDENTIFIER},
        manufacturer="Vistoda",
        model="Local Home Assistant adapter",
        name="Vistoda · BLINK Live",
    )


async def _status_or_retry(client: EngineClient) -> dict[str, object]:
    try:
        return await client.get_json("/v1/enrollment/status")
    except EngineError as error:
        raise ConfigEntryNotReady("Vistoda Blink engine is unavailable") from error


async def _discover_vistoda(hass: HomeAssistant) -> None:
    if any(
        entry.data.get("provider") == "blink"
        for entry in hass.config_entries.async_entries("media_bridge")
    ):
        return
    await hass.config_entries.flow.async_init(
        "media_bridge",
        context={"source": config_entries.SOURCE_INTEGRATION_DISCOVERY},
        data={"provider": "blink"},
    )
