"""Bounded, shared per-camera Blink live relay."""

import asyncio
import contextlib
import logging
from collections.abc import AsyncIterator
from typing import Any

from .const import (
    BATTERY_SESSION_SECONDS,
    POWERED_CAMERA_TYPES,
    POWERED_SESSION_SECONDS,
    QUEUE_DEPTH,
)
from .protocol import iter_mpegts

_LOGGER = logging.getLogger(__name__)
_END = object()


class CameraRelay:
    """Fan one Blink cloud session out to bounded local subscribers."""

    def __init__(self, camera: Any) -> None:
        self.camera = camera
        self._subscribers: set[asyncio.Queue[bytes | object]] = set()
        self._task: asyncio.Task[None] | None = None
        self._lock = asyncio.Lock()

    @property
    def active(self) -> bool:
        return self._task is not None and not self._task.done()

    async def subscribe(self) -> AsyncIterator[bytes]:
        queue: asyncio.Queue[bytes | object] = asyncio.Queue(maxsize=QUEUE_DEPTH)
        async with self._lock:
            self._subscribers.add(queue)
            if not self.active:
                self._task = asyncio.create_task(self._run(), name="blink-live-relay")
        try:
            while (chunk := await queue.get()) is not _END:
                yield chunk  # type: ignore[misc]
        finally:
            async with self._lock:
                self._subscribers.discard(queue)
                if not self._subscribers and self.active:
                    self._task.cancel()

    async def stop(self) -> None:
        if self.active:
            self._task.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await self._task

    async def _run(self) -> None:
        camera_type = str(getattr(self.camera, "camera_type", "")).casefold()
        deadline = (
            POWERED_SESSION_SECONDS
            if camera_type in POWERED_CAMERA_TYPES
            else BATTERY_SESSION_SECONDS
        )
        try:
            async with asyncio.timeout(deadline):
                live = await self.camera.init_livestream()
                async for chunk in iter_mpegts(live):
                    for queue in tuple(self._subscribers):
                        if queue.full():
                            queue.get_nowait()
                        queue.put_nowait(chunk)
        except asyncio.CancelledError:
            raise
        except TimeoutError:
            _LOGGER.debug("Blink live session reached its safety deadline")
        except Exception:
            _LOGGER.exception("Blink live relay failed")
        finally:
            for queue in tuple(self._subscribers):
                if queue.full():
                    queue.get_nowait()
                queue.put_nowait(_END)


class RelayManager:
    """Own relays for the loaded Blink coordinator."""

    def __init__(self, cameras: dict[str, Any]) -> None:
        self.cameras = cameras
        self.relays = {alias: CameraRelay(camera) for alias, camera in cameras.items()}

    async def stop(self) -> None:
        await asyncio.gather(*(relay.stop() for relay in self.relays.values()))
