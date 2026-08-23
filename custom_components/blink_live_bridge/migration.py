"""Optional one-time migration from a configured official Blink account."""

from homeassistant.core import HomeAssistant

from .client import EngineClient, EngineError


async def async_import_official_credentials(
    hass: HomeAssistant,
    client: EngineClient,
) -> bool:
    """Copy a refresh credential once; never retain or call the coordinator."""
    entries = hass.config_entries.async_entries("blink")
    if len(entries) != 1:
        return False
    attributes = dict(entries[0].data)
    required = ("refresh_token", "hardware_id", "region_id", "account_id")
    if any(not attributes.get(key) for key in required):
        return False
    payload = {
        key: str(attributes[key]) if attributes.get(key) is not None else None
        for key in (*required, "user_id", "username")
    }
    try:
        await client.post("/v1/enrollment/import", payload)
    except EngineError:
        return False
    return True
