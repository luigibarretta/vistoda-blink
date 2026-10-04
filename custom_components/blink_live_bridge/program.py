"""Blink arm/disarm program switches on the Sync Module device (ADR 0014).

Blink keeps running server-side schedules after the official app is removed;
these switches make them visible and let users turn them off from HA.
"""

from typing import Any

from homeassistant.components.switch import SwitchDeviceClass, SwitchEntity
from homeassistant.const import EntityCategory
from homeassistant.exceptions import HomeAssistantError
from homeassistant.util import dt as dt_util

from .client import EngineError
from .entity import BlinkNetworkEntity
from .program_schedule import merge_program, program_attributes
from .runtime import BridgeRuntime

MAX_PROGRAM_SWITCHES = 64


class BlinkProgramSwitch(BlinkNetworkEntity, SwitchEntity):
    """On while Blink would run the program's arm/disarm schedule."""

    _attr_device_class = SwitchDeviceClass.SWITCH
    _attr_entity_category = EntityCategory.CONFIG
    _attr_icon = "mdi:calendar-clock"

    def __init__(
        self, runtime: BridgeRuntime, network: dict[str, Any], program: dict[str, Any]
    ) -> None:
        super().__init__(runtime, network, runtime.coordinator, f"program-{program['id']}")
        self.program_id = str(program["id"])
        self._attr_name = f"Programma: {program.get('name') or self.program_id}"

    @property
    def program(self) -> dict[str, Any] | None:
        programs = (self.coordinator.data or {}).get("programs") or []
        return next(
            (
                item
                for item in programs
                if isinstance(item, dict)
                and str(item.get("id")) == self.program_id
                and str(item.get("network_id")) == str(self.network_id)
            ),
            None,
        )

    @property
    def available(self) -> bool:
        return super().available and self.program is not None

    @property
    def is_on(self) -> bool | None:
        program = self.program
        return bool(program.get("enabled")) if program is not None else None

    @property
    def extra_state_attributes(self) -> dict[str, Any]:
        attributes = program_attributes(self.program, dt_util.get_default_time_zone())
        return {**attributes, "network_id": self.network_id}

    async def async_turn_on(self, **kwargs: Any) -> None:
        del kwargs
        await self._set(True)

    async def async_turn_off(self, **kwargs: Any) -> None:
        del kwargs
        await self._set(False)

    async def _set(self, enabled: bool) -> None:
        try:
            program = await self.runtime.client.post(
                f"/v1/networks/{self.network_id}/programs/{self.program_id}/enabled",
                {"enabled": enabled},
            )
        except EngineError as error:
            raise HomeAssistantError("Blink did not confirm the program change") from error
        # The engine verified the change by reading it back: no Blink refresh.
        data = dict(self.coordinator.data or {})
        data["programs"] = merge_program(data.get("programs"), program)
        self.coordinator.async_set_updated_data(data)


def program_switches(runtime: BridgeRuntime, known: set[str]) -> list[BlinkProgramSwitch]:
    """Switches for programs not yet in ``known`` (``network:program`` keys), bounded."""
    networks = {str(network["id"]): network for network in runtime.networks}
    entities = []
    for program in (runtime.coordinator.data or {}).get("programs") or []:
        if not isinstance(program, dict) or not str(program.get("id", "")).isdigit():
            continue
        key = f"{program.get('network_id')}:{program['id']}"
        network = networks.get(str(program.get("network_id")))
        if network is None or key in known or len(known) >= MAX_PROGRAM_SWITCHES:
            continue
        known.add(key)
        entities.append(BlinkProgramSwitch(runtime, network, program))
    return entities
