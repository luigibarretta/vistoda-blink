"""Config flow for Blink Live Bridge."""

from typing import Any

from homeassistant import config_entries
from homeassistant.data_entry_flow import FlowResult
import voluptuous as vol

from .const import BLINK_DOMAIN, DOMAIN


class ConfigFlow(config_entries.ConfigFlow, domain=DOMAIN):
    """Create the single bridge instance after Blink is loaded."""

    VERSION = 1

    async def async_step_user(self, user_input: dict[str, Any] | None = None) -> FlowResult:
        await self.async_set_unique_id(DOMAIN)
        self._abort_if_unique_id_configured()
        if not self.hass.config_entries.async_loaded_entries(BLINK_DOMAIN):
            return self.async_abort(reason="blink_not_loaded")
        if user_input is None:
            return self.async_show_form(step_id="user", data_schema=vol.Schema({}))
        return self.async_create_entry(title="Blink Live Bridge", data={})
