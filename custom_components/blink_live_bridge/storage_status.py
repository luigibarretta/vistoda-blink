"""Periodic Sync Module USB status for the storage health entities."""

import logging
from datetime import timedelta
from typing import Any

from homeassistant.core import HomeAssistant
from homeassistant.helpers.update_coordinator import DataUpdateCoordinator, UpdateFailed

from .client import EngineClient, EngineError
from .const import DOMAIN

_LOGGER = logging.getLogger(__name__)
# One status-only Blink request per Sync Module; it never rebuilds the USB
# manifest. Five minutes notices a removed drive without hammering Blink.
STORAGE_INTERVAL = timedelta(minutes=5)
# The engine queries Blink once per Sync Module, each bounded to 20 s.
STORAGE_TIMEOUT = 60
MAX_STORAGES = 16


class BlinkStorageCoordinator(DataUpdateCoordinator[dict[str, dict[str, Any]]]):
    """Map network ID to its Sync Module USB status; empty with older providers."""

    def __init__(self, hass: HomeAssistant, client: EngineClient) -> None:
        super().__init__(
            hass, logger=_LOGGER, name=f"{DOMAIN}_storage", update_interval=STORAGE_INTERVAL
        )
        self.client = client
        self.supported = True

    async def _async_update_data(self) -> dict[str, dict[str, Any]]:
        if not self.supported:
            return {}
        try:
            result = await self.client.get_json("/v1/local-storage/status", STORAGE_TIMEOUT)
        except EngineError as error:
            if error.status == 404:
                # Providers before 0.19 have no status-only route.
                self.supported = False
                return {}
            raise UpdateFailed("Blink USB status is unavailable") from error
        storages = result.get("storages")
        if not isinstance(storages, list) or len(storages) > MAX_STORAGES:
            raise UpdateFailed("Blink USB status is invalid")
        return {
            str(storage["network_id"]): storage["status"]
            for storage in storages
            if isinstance(storage, dict)
            and storage.get("network_id") is not None
            and isinstance(storage.get("status"), dict)
        }
