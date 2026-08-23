"""Standalone Blink Sync Module alarm panels."""

from typing import Any, override

from homeassistant.components.alarm_control_panel import (
    AlarmControlPanelEntity,
    AlarmControlPanelEntityFeature,
    AlarmControlPanelState,
)
from homeassistant.config_entries import ConfigEntry
from homeassistant.core import HomeAssistant
from homeassistant.helpers.device_registry import DeviceInfo
from homeassistant.helpers.entity_platform import AddConfigEntryEntitiesCallback
from homeassistant.helpers.update_coordinator import CoordinatorEntity

from .const import DOMAIN, VISTODA_BLINK_IDENTIFIER, VISTODA_DOMAIN
from .runtime import BlinkCoordinator, BridgeRuntime


async def async_setup_entry(
    hass: HomeAssistant,
    entry: ConfigEntry,
    async_add_entities: AddConfigEntryEntitiesCallback,
) -> None:
    runtime: BridgeRuntime = hass.data[DOMAIN][entry.entry_id]
    async_add_entities(BlinkAlarm(runtime, network) for network in runtime.networks)


class BlinkAlarm(CoordinatorEntity[BlinkCoordinator], AlarmControlPanelEntity):
    """Arm or disarm a Blink network without an official integration."""

    _attr_supported_features = AlarmControlPanelEntityFeature.ARM_AWAY
    _attr_code_arm_required = False
    _attr_has_entity_name = True
    _attr_name = None

    def __init__(self, runtime: BridgeRuntime, network: dict[str, Any]) -> None:
        super().__init__(runtime.coordinator)
        self.runtime = runtime
        self.network_id = network["id"]
        serial = network.get("serial") or f"network-{self.network_id}"
        self._attr_unique_id = f"vistoda-{serial}"
        self._attr_device_info = DeviceInfo(
            identifiers={(DOMAIN, serial)},
            serial_number=network.get("serial"),
            sw_version=network.get("firmware"),
            name=network["name"],
            manufacturer="Blink",
            via_device=(VISTODA_DOMAIN, VISTODA_BLINK_IDENTIFIER),
        )

    @property
    def network(self) -> dict[str, Any]:
        return next(
            item for item in self.coordinator.data["networks"] if item["id"] == self.network_id
        )

    @property
    @override
    def alarm_state(self) -> AlarmControlPanelState:
        return (
            AlarmControlPanelState.ARMED_AWAY
            if self.network.get("armed")
            else AlarmControlPanelState.DISARMED
        )

    async def async_alarm_disarm(self, code: str | None = None) -> None:
        del code
        await self._armed(False)

    async def async_alarm_arm_away(self, code: str | None = None) -> None:
        del code
        await self._armed(True)

    async def _armed(self, value: bool) -> None:
        await self.runtime.client.post(f"/v1/networks/{self.network_id}/armed", {"armed": value})
        await self.coordinator.async_request_refresh()
