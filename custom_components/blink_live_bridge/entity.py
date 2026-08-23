"""Shared standalone Vistoda Blink entity helpers."""

from typing import Any

from homeassistant.helpers.device_registry import DeviceInfo
from homeassistant.helpers.update_coordinator import CoordinatorEntity

from .const import DOMAIN, VISTODA_BLINK_IDENTIFIER, VISTODA_DOMAIN
from .runtime import BlinkCoordinator


class BlinkCameraEntity(CoordinatorEntity[BlinkCoordinator]):
    """Entity bound to a provider camera by stable alias."""

    _attr_has_entity_name = True

    def __init__(self, coordinator: BlinkCoordinator, camera: dict[str, Any], suffix: str) -> None:
        super().__init__(coordinator)
        self.alias = camera["alias"]
        self._attr_unique_id = f"{identity(camera)}-{suffix}"
        self._attr_device_info = camera_device(camera)

    @property
    def camera(self) -> dict[str, Any]:
        return next(
            camera for camera in self.coordinator.data["cameras"] if camera["alias"] == self.alias
        )


def identity(camera: dict[str, Any]) -> str:
    """Prefer the physical serial while tolerating incomplete devices."""
    return str(camera.get("serial") or f"camera-{camera['id']}")


def camera_device(camera: dict[str, Any]) -> DeviceInfo:
    """Expose official-equivalent camera metadata under Vistoda's domain."""
    serial = identity(camera)
    return DeviceInfo(
        identifiers={(DOMAIN, serial)},
        serial_number=camera.get("serial"),
        sw_version=camera.get("firmware"),
        name=camera["name"],
        manufacturer="Blink",
        model=camera.get("camera_type"),
        via_device=(VISTODA_DOMAIN, VISTODA_BLINK_IDENTIFIER),
    )
