"""Fast motion state from the provider's cached v4 event poller (ADR 0012)."""

import logging
from datetime import timedelta
from typing import Any

from homeassistant.core import HomeAssistant
from homeassistant.helpers.update_coordinator import DataUpdateCoordinator, UpdateFailed

from .client import EngineClient, EngineError
from .const import DOMAIN

_LOGGER = logging.getLogger(__name__)
# The provider answers from memory, so a short interval costs no Blink request.
MOTION_INTERVAL = timedelta(seconds=15)


class BlinkMotionCoordinator(DataUpdateCoordinator[dict[str, dict[str, Any]]]):
    """Map camera alias to its latest motion state; empty with older providers."""

    def __init__(self, hass: HomeAssistant, client: EngineClient) -> None:
        super().__init__(
            hass, logger=_LOGGER, name=f"{DOMAIN}_motion", update_interval=MOTION_INTERVAL
        )
        self.client = client
        self.supported = True

    async def _async_update_data(self) -> dict[str, dict[str, Any]]:
        if not self.supported:
            return {}
        try:
            result = await self.client.get_json("/v1/motion")
        except EngineError as error:
            if error.status == 404:
                # Providers before 0.20 have no motion route: keep the clip-based state.
                self.supported = False
                return {}
            raise UpdateFailed("Blink motion state is unavailable") from error
        cameras = result.get("cameras")
        if not isinstance(cameras, list):
            raise UpdateFailed("Blink motion state is invalid")
        return {
            camera["alias"]: camera
            for camera in cameras
            if isinstance(camera, dict) and isinstance(camera.get("alias"), str)
        }
