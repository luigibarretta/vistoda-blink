"""Redacted Vistoda Blink diagnostics."""

from typing import Any

from homeassistant.components.diagnostics import async_redact_data
from homeassistant.config_entries import ConfigEntry
from homeassistant.core import HomeAssistant

from .const import DOMAIN
from .runtime import BridgeRuntime

TO_REDACT = {
    "account_id",
    "media_url",
    "serial",
    "thumbnail_url",
    "token",
    "unique_id",
    "username",
}


async def async_get_config_entry_diagnostics(
    hass: HomeAssistant,
    entry: ConfigEntry,
) -> dict[str, Any]:
    """Expose state and version only; credentials never enter Home Assistant."""
    runtime: BridgeRuntime = hass.data[DOMAIN][entry.entry_id]
    state = runtime.coordinator.data
    return async_redact_data(
        {
            "standalone": True,
            "entry": {"version": entry.version, "options": dict(entry.options)},
            "account": {"enrolled": bool(state.get("account_id"))},
            "networks": state.get("networks", []),
            "cameras": state.get("cameras", []),
            "clips": {"count": len(state.get("clips", []))},
        },
        TO_REDACT,
    )
