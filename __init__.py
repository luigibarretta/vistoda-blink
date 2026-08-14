"""Blink Live Bridge setup."""

import voluptuous as vol

from homeassistant.config_entries import ConfigEntry
from homeassistant.core import HomeAssistant
from homeassistant.helpers import config_validation as cv
from homeassistant.helpers.typing import ConfigType

from .const import CONF_TOKEN, DOMAIN, PLATFORMS
from .http import register_views
from .relay import RelayManager
from .runtime import BridgeRuntime, camera_aliases, loaded_blink_coordinator

CONFIG_SCHEMA = vol.Schema(
    {
        vol.Required(DOMAIN): vol.Schema(
            {vol.Required(CONF_TOKEN): vol.Match(r"^[0-9a-f]{64}$")}
        )
    },
    extra=vol.ALLOW_EXTRA,
)


async def async_setup(hass: HomeAssistant, config: ConfigType) -> bool:
    """Store the workload token before the config entry is created."""
    hass.data.setdefault(DOMAIN, {})[CONF_TOKEN] = config[DOMAIN][CONF_TOKEN]
    return True


async def async_setup_entry(hass: HomeAssistant, entry: ConfigEntry) -> bool:
    """Reuse the loaded built-in Blink coordinator."""
    data = hass.data.setdefault(DOMAIN, {})
    coordinator = loaded_blink_coordinator(hass)
    cameras = camera_aliases(coordinator)
    runtime = BridgeRuntime(
        coordinator=coordinator,
        cameras=cameras,
        relays=RelayManager(cameras),
        token=data[CONF_TOKEN],
    )
    data["runtime"] = runtime
    if not data.get("views_registered"):
        register_views(hass)
        data["views_registered"] = True
    await hass.config_entries.async_forward_entry_setups(entry, PLATFORMS)
    return True


async def async_unload_entry(hass: HomeAssistant, entry: ConfigEntry) -> bool:
    """Stop cloud sessions before unloading entities."""
    if not await hass.config_entries.async_unload_platforms(entry, PLATFORMS):
        return False
    runtime: BridgeRuntime | None = hass.data[DOMAIN].pop("runtime", None)
    if runtime:
        await runtime.relays.stop()
    return True
