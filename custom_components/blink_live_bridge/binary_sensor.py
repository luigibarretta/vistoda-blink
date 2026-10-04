"""Standalone Blink diagnostic and motion binary sensors."""

from dataclasses import dataclass
from typing import Any

from homeassistant.components.binary_sensor import BinarySensorDeviceClass, BinarySensorEntity
from homeassistant.config_entries import ConfigEntry
from homeassistant.const import EntityCategory
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddConfigEntryEntitiesCallback

from .const import API_PREFIX, DOMAIN
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
    Description("temperature_out_of_range", "Temperatura fuori soglia"),
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
        super().__init__(runtime, camera, description.key)
        self.key = description.key
        self._attr_name = description.name
        self._attr_device_class = description.device_class
        self._attr_entity_category = EntityCategory.DIAGNOSTIC if description.diagnostic else None
        self._attr_entity_registry_enabled_default = description.enabled

    async def async_added_to_hass(self) -> None:
        await super().async_added_to_hass()
        if self.key == "motion_detected" and self.runtime.motion is not None:
            self.async_on_remove(self.runtime.motion.async_add_listener(self.async_write_ha_state))

    @property
    def _motion(self) -> dict[str, Any] | None:
        motion = self.runtime.motion
        return motion.data.get(self.alias) if motion is not None and motion.data else None

    @property
    def is_on(self) -> bool | None:
        if self.key == "motion_detected" and self._motion is not None:
            return bool(self._motion.get("motion_detected"))
        value = self.camera.get(self.key)
        return bool(value) if value is not None else None

    @property
    def extra_state_attributes(self) -> dict[str, Any] | None:
        if self.key == "motion_detected":
            motion = self._motion or {}
            attributes = {
                key: motion.get(key)
                for key in ("last_motion_at", "event_type", "has_media", "event_id")
            }
            # Relative HA URL: Companion notifications fetch it with the user's session.
            event_id = motion.get("event_id")
            attributes["thumbnail"] = (
                f"{API_PREFIX}/motion/{event_id}/thumbnail.jpg"
                if motion.get("thumbnail_available") and str(event_id or "").isdigit()
                else None
            )
            return attributes
        if self.key != "temperature_out_of_range":
            return None
        return {
            "alerts_enabled": self.camera.get("temperature_alerts"),
            "temperature_f": self.camera.get("temperature_f"),
            "temperature_min_f": self.camera.get("temperature_min_f"),
            "temperature_max_f": self.camera.get("temperature_max_f"),
            "alias": self.camera.get("alias"),
        }
