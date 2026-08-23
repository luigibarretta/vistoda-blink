"""Optional one-time migration from a configured official Blink account."""

from typing import Any

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
    if runtime_attributes := _login_attributes(entries[0].runtime_data):
        attributes.update(runtime_attributes)
    required = ("refresh_token", "hardware_id", "region_id", "account_id")
    if any(not attributes.get(key) for key in required):
        return False
    payload = {key: attributes.get(key) for key in (*required, "user_id", "username")}
    try:
        await client.post("/v1/enrollment/import", payload)
    except EngineError:
        return False
    return True


def _login_attributes(runtime: Any) -> dict[str, Any] | None:
    """Read provider values through guarded generic access."""
    api = getattr(runtime, "api", None)
    auth = getattr(api, "auth", None)
    value = getattr(auth, "login_attributes", None)
    return value if isinstance(value, dict) else None
