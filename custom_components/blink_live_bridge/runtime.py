"""Home Assistant runtime backed only by the standalone Rust provider."""

import logging
from dataclasses import dataclass
from datetime import timedelta
from typing import Any

from homeassistant.core import HomeAssistant
from homeassistant.exceptions import ConfigEntryAuthFailed
from homeassistant.helpers.update_coordinator import DataUpdateCoordinator, UpdateFailed

from .client import EngineClient, EngineError
from .const import DEFAULT_SCAN_INTERVAL, DOMAIN

_LOGGER = logging.getLogger(__name__)


class BlinkCoordinator(DataUpdateCoordinator[dict[str, Any]]):
    """Poll the provider state without knowing Blink's protocol."""

    def __init__(self, hass: HomeAssistant, client: EngineClient, scan_interval: int) -> None:
        super().__init__(
            hass,
            logger=_LOGGER,
            name=DOMAIN,
            update_interval=timedelta(seconds=scan_interval),
        )
        self.client = client

    async def _async_update_data(self) -> dict[str, Any]:
        try:
            await self.client.post("/v1/refresh")
            return await self.client.get_json("/v1/state")
        except EngineError as error:
            if error.status == 401:
                raise ConfigEntryAuthFailed("Blink authorization expired") from error
            raise UpdateFailed("Standalone Blink provider is unavailable") from error


@dataclass(slots=True)
class BridgeRuntime:
    """Objects shared by all Vistoda Blink platforms and proxy views."""

    client: EngineClient
    coordinator: BlinkCoordinator
    token: str

    @property
    def cameras(self) -> list[dict[str, Any]]:
        return self.coordinator.data.get("cameras", [])

    @property
    def networks(self) -> list[dict[str, Any]]:
        return self.coordinator.data.get("networks", [])


def scan_interval(options: dict[str, Any]) -> int:
    """Return a bounded refresh interval."""
    return max(30, min(3600, int(options.get("scan_interval", DEFAULT_SCAN_INTERVAL))))
