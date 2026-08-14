"""Runtime discovery and stable camera aliases."""

from dataclasses import dataclass
from typing import Any

from homeassistant.core import HomeAssistant
from homeassistant.exceptions import ConfigEntryNotReady
from homeassistant.util import slugify

from .const import BLINK_DOMAIN
from .relay import RelayManager


@dataclass(slots=True)
class BridgeRuntime:
    """Loaded Blink coordinator plus the shared relays."""

    coordinator: Any
    cameras: dict[str, Any]
    relays: RelayManager
    token: str


def camera_aliases(coordinator: Any) -> dict[str, Any]:
    """Build stable, collision-safe aliases without exposing serials."""
    result: dict[str, Any] = {}
    for name, camera in coordinator.api.cameras.items():
        base = slugify(name) or "camera"
        alias = base
        suffix = 2
        while alias in result:
            alias = f"{base}-{suffix}"
            suffix += 1
        result[alias] = camera
    return result


def loaded_blink_coordinator(hass: HomeAssistant) -> Any:
    """Return the single loaded built-in Blink coordinator."""
    entries = hass.config_entries.async_loaded_entries(BLINK_DOMAIN)
    if len(entries) != 1 or entries[0].runtime_data is None:
        raise ConfigEntryNotReady("Exactly one loaded Blink account is required")
    return entries[0].runtime_data
