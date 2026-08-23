"""Standalone Blink motion detection switches."""

from typing import Any

from homeassistant.components.switch import SwitchDeviceClass, SwitchEntity
from homeassistant.config_entries import ConfigEntry
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddConfigEntryEntitiesCallback

from .const import DOMAIN
from .entity import BlinkCameraEntity
from .runtime import BridgeRuntime


async def async_setup_entry(
    hass: HomeAssistant,
    entry: ConfigEntry,
    async_add_entities: AddConfigEntryEntitiesCallback,
) -> None:
    runtime: BridgeRuntime = hass.data[DOMAIN][entry.entry_id]
    async_add_entities(BlinkMotionSwitch(runtime, camera) for camera in runtime.cameras)


class BlinkMotionSwitch(BlinkCameraEntity, SwitchEntity):
    """Enable or disable motion recording directly through Vistoda."""

    _attr_name = "Rilevamento movimento"
    _attr_device_class = SwitchDeviceClass.SWITCH

    def __init__(self, runtime: BridgeRuntime, camera: dict[str, Any]) -> None:
        super().__init__(runtime.coordinator, camera, "motion_enabled")
        self.runtime = runtime

    @property
    def is_on(self) -> bool | None:
        value = self.camera.get("enabled")
        return bool(value) if value is not None else None

    async def async_turn_on(self, **kwargs: Any) -> None:
        del kwargs
        await self._set(True)

    async def async_turn_off(self, **kwargs: Any) -> None:
        del kwargs
        await self._set(False)

    async def _set(self, enabled: bool) -> None:
        await self.runtime.client.post(
            f"/v1/cameras/{self.alias}/commands",
            {"action": "motion", "enabled": enabled},
        )
        await self.coordinator.async_request_refresh()
