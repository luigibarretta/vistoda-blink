"""Fast motion state from the provider's cached v4 event poller (ADR 0012, 0015)."""

import asyncio
import logging
import time
from datetime import timedelta
from typing import Any

from homeassistant.core import HomeAssistant
from homeassistant.helpers.update_coordinator import DataUpdateCoordinator, UpdateFailed

from .client import EngineClient, EngineError
from .const import DOMAIN

_LOGGER = logging.getLogger(__name__)
# The provider answers from memory, so a short interval costs no Blink request.
MOTION_INTERVAL = timedelta(seconds=15)
# Long-poll: the engine holds the request until the motion state changes.
LONG_POLL_WAIT = 25
LONG_POLL_TIMEOUT = 40
LONG_POLL_RETRY = 15
# A long-poll that returns this fast without a change is not honoured.
LONG_POLL_MIN_SECONDS = 1


class BlinkMotionCoordinator(DataUpdateCoordinator[dict[str, dict[str, Any]]]):
    """Map camera alias to its latest motion state; empty with older providers."""

    def __init__(self, hass: HomeAssistant, client: EngineClient) -> None:
        super().__init__(
            hass, logger=_LOGGER, name=f"{DOMAIN}_motion", update_interval=MOTION_INTERVAL
        )
        self.client = client
        self.supported = True
        self.sequence: int | None = None

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
        return self._parse(result)

    def _parse(self, result: dict[str, Any]) -> dict[str, dict[str, Any]]:
        cameras = result.get("cameras")
        if not isinstance(cameras, list):
            raise UpdateFailed("Blink motion state is invalid")
        sequence = result.get("sequence")
        # Providers before 0.23 send no sequence and are polled only.
        self.sequence = sequence if isinstance(sequence, int) and sequence >= 0 else None
        return {
            camera["alias"]: camera
            for camera in cameras
            if isinstance(camera, dict) and isinstance(camera.get("alias"), str)
        }

    async def async_long_poll(self) -> None:
        """Push a new event or still into HA as soon as the engine sees it.

        The engine answers from memory (no Blink request); the 15-second
        interval refresh stays as a fallback and expires the motion window.
        """
        while self.supported:
            since = self.sequence
            if since is None:
                if self.last_update_success:
                    return  # A healthy engine without sequences predates 0.23.
                # The setup refresh failed: wait for a refresh to learn the sequence.
                await asyncio.sleep(LONG_POLL_RETRY)
                continue
            started = time.monotonic()
            try:
                result = await self.client.get_json(
                    f"/v1/motion?since={since}&wait={LONG_POLL_WAIT}", LONG_POLL_TIMEOUT
                )
                data = self._parse(result)
            except (EngineError, UpdateFailed):
                await asyncio.sleep(LONG_POLL_RETRY)
                continue
            if self.sequence is None:
                return
            if self.sequence != since:
                self.async_set_updated_data(data)
            elif time.monotonic() - started < LONG_POLL_MIN_SECONDS:
                await asyncio.sleep(LONG_POLL_RETRY)
