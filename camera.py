"""Native Blink live camera entities."""

from typing import Any, override

from homeassistant.components.camera import Camera, CameraEntityFeature
from homeassistant.config_entries import ConfigEntry
from homeassistant.core import HomeAssistant
from homeassistant.helpers.entity_platform import AddConfigEntryEntitiesCallback
from homeassistant.helpers.update_coordinator import CoordinatorEntity

from .const import API_PREFIX, DOMAIN
from .runtime import BridgeRuntime


async def async_setup_entry(
    hass: HomeAssistant,
    entry: ConfigEntry,
    async_add_entities: AddConfigEntryEntitiesCallback,
) -> None:
    """Create one live companion entity for each built-in Blink camera."""
    runtime: BridgeRuntime = hass.data[DOMAIN]["runtime"]
    async_add_entities(
        BlinkLiveCamera(runtime.coordinator, alias, camera)
        for alias, camera in runtime.cameras.items()
    )


class BlinkLiveCamera(CoordinatorEntity[Any], Camera):
    """Blink snapshot plus on-demand MPEG-TS live view."""

    _attr_has_entity_name = False
    _attr_supported_features = CameraEntityFeature.STREAM

    def __init__(self, coordinator: Any, alias: str, camera: Any) -> None:
        super().__init__(coordinator)
        Camera.__init__(self)
        self._alias = alias
        self._camera = camera
        self._attr_name = f"{camera.name} Live"
        self._attr_suggested_object_id = f"{alias}_live"
        self._attr_unique_id = f"{camera.serial}-live-camera"

    @property
    @override
    def available(self) -> bool:
        return super().available and self._camera is not None

    @override
    def camera_image(
        self, width: int | None = None, height: int | None = None
    ) -> bytes | None:
        return self._camera.image_from_cache

    @override
    async def stream_source(self) -> str:
        return f"http://127.0.0.1:8123{API_PREFIX}/v1/cameras/{self._alias}/live.ts"
