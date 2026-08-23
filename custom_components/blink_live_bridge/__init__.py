"""Standalone Vistoda Blink setup with stable Home Assistant contracts."""

import voluptuous as vol
from homeassistant import config_entries
from homeassistant.config_entries import ConfigEntry
from homeassistant.core import HomeAssistant
from homeassistant.exceptions import ConfigEntryNotReady
from homeassistant.helpers.typing import ConfigType

from .client import EngineClient, EngineError
from .const import CONF_TOKEN, CONF_URL, DOMAIN, ENGINE_URL, PLATFORMS
from .http import register_views
from .migration import async_import_official_credentials
from .runtime import BlinkCoordinator, BridgeRuntime, scan_interval
from .services import async_setup_services

CONFIG_SCHEMA = vol.Schema(
    {vol.Optional(DOMAIN): vol.Schema({vol.Required(CONF_TOKEN): vol.Match(r"^[0-9a-f]{64}$")})},
    extra=vol.ALLOW_EXTRA,
)


async def async_setup(hass: HomeAssistant, config: ConfigType) -> bool:
    """Store only the local workload token."""
    async_setup_services(hass)
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
    if not status.get("enrolled") and not await async_import_official_credentials(hass, client):
        raise ConfigEntryNotReady("Vistoda Blink requires standalone enrollment")
    coordinator = BlinkCoordinator(hass, client, scan_interval(entry.options))
    await coordinator.async_config_entry_first_refresh()
    data[entry.entry_id] = BridgeRuntime(client, coordinator, token)
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
    hass.data[DOMAIN].pop(entry.entry_id, None)
    hass.data[DOMAIN].pop("runtime", None)
    return True


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
