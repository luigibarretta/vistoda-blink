"""Standalone Blink diagnostic and motion binary sensors."""

from dataclasses import dataclass
from typing import Any

from homeassistant.components.binary_sensor import BinarySensorDeviceClass, BinarySensorEntity
from homeassistant.config_entries import ConfigEntry
from homeassistant.const import EntityCategory
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddConfigEntryEntitiesCallback

from .const import DOMAIN
from .entity import BlinkCameraEntity
from .runtime import BridgeRuntime


@dataclass(frozen=True, slots=True)
class Description:
    key: str
    name: str
    device_class: BinarySensorDeviceClass | None = None
    diagnostic: bool = False
    enabled: bool = True


DESCRIPTIONS = (
    Description("low_battery", "Batteria scarica", BinarySensorDeviceClass.BATTERY, True),
    Description("enabled", "Camera armata", enabled=False),
    Description("motion_detected", "Movimento", BinarySensorDeviceClass.MOTION),
)


async def async_setup_entry(
    hass: HomeAssistant,
    entry: ConfigEntry,
    async_add_entities: AddConfigEntryEntitiesCallback,
) -> None:
    runtime: BridgeRuntime = hass.data[DOMAIN][entry.entry_id]
    async_add_entities(
        BlinkBinarySensor(runtime, camera, description)
        for camera in runtime.cameras
        for description in DESCRIPTIONS
    )


class BlinkBinarySensor(BlinkCameraEntity, BinarySensorEntity):
    """Official-parity camera binary sensor."""

    def __init__(
        self, runtime: BridgeRuntime, camera: dict[str, Any], description: Description
    ) -> None:
        super().__init__(runtime.coordinator, camera, description.key)
        self.key = description.key
        self._attr_name = description.name
        self._attr_device_class = description.device_class
        self._attr_entity_category = EntityCategory.DIAGNOSTIC if description.diagnostic else None
        self._attr_entity_registry_enabled_default = description.enabled

    @property
    def is_on(self) -> bool | None:
        value = self.camera.get(self.key)
        return bool(value) if value is not None else None
