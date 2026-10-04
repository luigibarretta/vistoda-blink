"""Sync Module connectivity and USB storage health entities (ADR 0013)."""

from typing import Any

from homeassistant.components.binary_sensor import BinarySensorDeviceClass, BinarySensorEntity
from homeassistant.components.sensor import SensorDeviceClass, SensorEntity

from .entity import BlinkNetworkEntity, sync_module_networks
from .runtime import BridgeRuntime
from .storage_status import BlinkStorageCoordinator
from .usb_health import USB_STATES, usb_attributes, usb_problem, usb_state


class SyncModuleConnectivity(BlinkNetworkEntity, BinarySensorEntity):
    """On while Blink reports the Sync Module online; unknown statuses stay unknown."""

    _attr_name = "Connessione"
    _attr_device_class = BinarySensorDeviceClass.CONNECTIVITY

    def __init__(self, runtime: BridgeRuntime, network: dict[str, Any]) -> None:
        super().__init__(runtime, network, runtime.coordinator, "online")

    @property
    def is_on(self) -> bool | None:
        value = self.network.get("online")
        return bool(value) if value is not None else None

    @property
    def extra_state_attributes(self) -> dict[str, Any]:
        return {"status": self.network.get("status"), "network_id": self.network_id}


class UsbStorageEntity(BlinkNetworkEntity):
    """Entity bound to one Sync Module's periodically refreshed USB status."""

    @property
    def status(self) -> dict[str, Any] | None:
        data = self.coordinator.data or {}
        return data.get(str(self.network_id))

    @property
    def available(self) -> bool:
        return super().available and self.status is not None

    @property
    def extra_state_attributes(self) -> dict[str, Any]:
        return {**usb_attributes(self.status), "network_id": self.network_id}


class UsbStorageSensor(UsbStorageEntity, SensorEntity):
    """Normalized USB drive state; unknown when Blink gives no usable USB info."""

    _attr_name = "Archivio USB"
    _attr_device_class = SensorDeviceClass.ENUM

    def __init__(
        self, runtime: BridgeRuntime, network: dict[str, Any], storage: BlinkStorageCoordinator
    ) -> None:
        super().__init__(runtime, network, storage, "usb_storage")
        self._attr_options = list(USB_STATES)

    @property
    def native_value(self) -> str | None:
        return usb_state(self.status)


class UsbStorageProblem(UsbStorageEntity, BinarySensorEntity):
    """On when the USB drive needs attention (mapping in ADR 0013)."""

    _attr_name = "Problema archivio USB"
    _attr_device_class = BinarySensorDeviceClass.PROBLEM

    def __init__(
        self, runtime: BridgeRuntime, network: dict[str, Any], storage: BlinkStorageCoordinator
    ) -> None:
        super().__init__(runtime, network, storage, "usb_storage_problem")

    @property
    def is_on(self) -> bool | None:
        return usb_problem(self.status)


def network_binary_sensors(runtime: BridgeRuntime) -> list[BinarySensorEntity]:
    """One connectivity sensor per Sync Module, plus USB health when polled."""
    entities: list[BinarySensorEntity] = []
    for network in sync_module_networks(runtime):
        entities.append(SyncModuleConnectivity(runtime, network))
        if runtime.storage is not None:
            entities.append(UsbStorageProblem(runtime, network, runtime.storage))
    return entities


def network_sensors(runtime: BridgeRuntime) -> list[SensorEntity]:
    """One USB state sensor per Sync Module when storage status is polled."""
    storage = runtime.storage
    if storage is None:
        return []
    return [
        UsbStorageSensor(runtime, network, storage) for network in sync_module_networks(runtime)
    ]
