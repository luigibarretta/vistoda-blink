"""Standalone Blink temperature and Wi-Fi sensors."""

from dataclasses import dataclass
from typing import Any

from homeassistant.components.sensor import SensorDeviceClass, SensorEntity, SensorStateClass
from homeassistant.config_entries import ConfigEntry
from homeassistant.const import (
    SIGNAL_STRENGTH_DECIBELS_MILLIWATT,
    EntityCategory,
    UnitOfTemperature,
)
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddConfigEntryEntitiesCallback

from .const import DOMAIN
from .entity import BlinkCameraEntity
from .runtime import BridgeRuntime


@dataclass(frozen=True, slots=True)
class Description:
    key: str
    name: str
    unit: str
    device_class: SensorDeviceClass


DESCRIPTIONS = (
    Description(
        "temperature_f", "Temperatura", UnitOfTemperature.FAHRENHEIT, SensorDeviceClass.TEMPERATURE
    ),
    Description(
        "wifi_dbm",
        "Segnale Wi-Fi",
        SIGNAL_STRENGTH_DECIBELS_MILLIWATT,
        SensorDeviceClass.SIGNAL_STRENGTH,
    ),
)


async def async_setup_entry(
    hass: HomeAssistant,
    entry: ConfigEntry,
    async_add_entities: AddConfigEntryEntitiesCallback,
) -> None:
    runtime: BridgeRuntime = hass.data[DOMAIN][entry.entry_id]
    async_add_entities(
        BlinkSensor(runtime, camera, description)
        for camera in runtime.cameras
        for description in DESCRIPTIONS
    )


class BlinkSensor(BlinkCameraEntity, SensorEntity):
    """Official-parity camera measurement."""

    _attr_entity_category = EntityCategory.DIAGNOSTIC
    _attr_state_class = SensorStateClass.MEASUREMENT

    def __init__(
        self, runtime: BridgeRuntime, camera: dict[str, Any], description: Description
    ) -> None:
        super().__init__(runtime.coordinator, camera, description.key)
        self.key = description.key
        self._attr_name = description.name
        self._attr_native_unit_of_measurement = description.unit
        self._attr_device_class = description.device_class

    @property
    def native_value(self) -> float | int | None:
        return self.camera.get(self.key)
