"""Standalone Blink enrollment and options flow."""

from typing import Any

import voluptuous as vol
from homeassistant import config_entries
from homeassistant.data_entry_flow import FlowResult
from homeassistant.helpers.service_info.hassio import HassioServiceInfo

from .client import EngineClient, EngineError
from .const import (
    CONF_MANAGED_APP,
    CONF_SCAN_INTERVAL,
    CONF_TOKEN,
    CONF_URL,
    DEFAULT_SCAN_INTERVAL,
    DOMAIN,
    ENGINE_URL,
)


class ConfigFlow(config_entries.ConfigFlow, domain=DOMAIN):
    """Enroll directly with Blink through the local Rust provider."""

    # Keep the existing config-entry contract; enrollment moved behind it.
    VERSION = 1

    def __init__(self) -> None:
        self._client: EngineClient | None = None
        self._enrollment_id: str | None = None
        self._entry_data: dict[str, Any] = {}

    async def async_step_user(self, user_input: dict[str, Any] | None = None) -> FlowResult:
        await self.async_set_unique_id(DOMAIN)
        self._abort_if_unique_id_configured()
        return await self._credentials_form(user_input)

    async def async_step_hassio(self, discovery_info: HassioServiceInfo) -> FlowResult:
        """Adopt the supervised engine without YAML or a user-visible token."""
        config = discovery_info.config
        self._entry_data = {
            CONF_TOKEN: str(config.get(CONF_TOKEN, "")),
            CONF_URL: str(config.get(CONF_URL, "")),
            CONF_MANAGED_APP: True,
        }
        self._client = EngineClient(
            self.hass, self._entry_data[CONF_TOKEN], self._entry_data[CONF_URL]
        )
        await self.async_set_unique_id(DOMAIN)
        self._abort_if_unique_id_configured(updates=self._entry_data)
        return await self._credentials_form(None)

    async def async_step_reauth(self, entry_data: dict[str, Any]) -> FlowResult:
        """Replace an expired standalone Blink authorization."""
        self._entry_data = dict(entry_data)
        return await self.async_step_reauth_confirm()

    async def async_step_reauth_confirm(
        self,
        user_input: dict[str, Any] | None = None,
    ) -> FlowResult:
        return await self._credentials_form(user_input, reauth=True)

    async def _credentials_form(
        self,
        user_input: dict[str, Any] | None,
        *,
        reauth: bool = False,
    ) -> FlowResult:
        client = self._engine()
        errors: dict[str, str] = {}
        if user_input is None and not reauth:
            try:
                if (await client.get_json("/v1/enrollment/status")).get("enrolled"):
                    return self.async_show_form(step_id="confirm", data_schema=vol.Schema({}))
            except EngineError:
                errors["base"] = "engine_unavailable"
        elif user_input is not None:
            try:
                result = await client.post("/v1/enrollment/start", user_input)
                if result.get("status") == "enrolled":
                    return self._finish()
                self._enrollment_id = result["enrollment_id"]
                return await self.async_step_two_factor()
            except (EngineError, KeyError):
                errors["base"] = "cannot_connect"
        return self.async_show_form(
            step_id="reauth_confirm" if reauth else "user",
            data_schema=vol.Schema({vol.Required("username"): str, vol.Required("password"): str}),
            errors=errors,
        )

    async def async_step_confirm(self, user_input: dict[str, Any] | None = None) -> FlowResult:
        if user_input is None:
            return self.async_show_form(step_id="confirm", data_schema=vol.Schema({}))
        return self._entry()

    async def async_step_two_factor(
        self,
        user_input: dict[str, Any] | None = None,
    ) -> FlowResult:
        errors: dict[str, str] = {}
        if user_input is not None and self._enrollment_id:
            try:
                await self._engine().post(
                    f"/v1/enrollment/{self._enrollment_id}/complete",
                    user_input,
                )
                return self._finish()
            except EngineError:
                errors["base"] = "invalid_two_factor"
        return self.async_show_form(
            step_id="two_factor",
            data_schema=vol.Schema({vol.Required("code"): str}),
            errors=errors,
        )

    @staticmethod
    def async_get_options_flow(config_entry: config_entries.ConfigEntry) -> "OptionsFlow":
        return OptionsFlow(config_entry)

    def _engine(self) -> EngineClient:
        if self._client is None:
            data = self.hass.data.get(DOMAIN, {})
            token = self._entry_data.get(CONF_TOKEN) or data.get(CONF_TOKEN, "")
            url = self._entry_data.get(CONF_URL, ENGINE_URL)
            self._client = EngineClient(self.hass, token, url)
        return self._client

    def _entry(self) -> FlowResult:
        return self.async_create_entry(title="Vistoda · Blink", data=self._entry_data)

    def _finish(self) -> FlowResult:
        entry_id = self.context.get("entry_id")
        if not entry_id:
            return self._entry()
        entry = self.hass.config_entries.async_get_entry(entry_id)
        if entry is not None:
            return self.async_update_reload_and_abort(entry, data_updates=self._entry_data)
        return self.async_abort(reason="reauth_successful")


class OptionsFlow(config_entries.OptionsFlow):
    """Configure the standalone provider refresh cadence."""

    def __init__(self, entry: config_entries.ConfigEntry) -> None:
        self._entry = entry

    async def async_step_init(self, user_input: dict[str, Any] | None = None) -> FlowResult:
        if user_input is not None:
            return self.async_create_entry(title="", data=user_input)
        current = self._entry.options.get(CONF_SCAN_INTERVAL, DEFAULT_SCAN_INTERVAL)
        return self.async_show_form(
            step_id="init",
            data_schema=vol.Schema(
                {
                    vol.Required(CONF_SCAN_INTERVAL, default=current): vol.All(
                        int, vol.Range(min=30, max=3600)
                    )
                }
            ),
        )
