"""Standalone Blink snapshot and live camera entities."""

from pathlib import Path
from typing import Any, override

from homeassistant.components.camera import Camera, CameraEntityFeature
from homeassistant.config_entries import ConfigEntry
from homeassistant.core import HomeAssistant, callback
from homeassistant.exceptions import HomeAssistantError, ServiceValidationError
from homeassistant.helpers.entity_platform import AddConfigEntryEntitiesCallback

from .client import EngineError
from .const import API_PREFIX, DOMAIN
from .entity import BlinkCameraEntity, camera_device, identity
from .runtime import BridgeRuntime


async def async_setup_entry(
    hass: HomeAssistant,
    entry: ConfigEntry,
    async_add_entities: AddConfigEntryEntitiesCallback,
) -> None:
    """Create one compatible snapshot/live camera per provider device."""
    runtime: BridgeRuntime = hass.data[DOMAIN][entry.entry_id]
    async_add_entities(BlinkLiveCamera(runtime, camera) for camera in runtime.cameras)


class BlinkLiveCamera(BlinkCameraEntity, Camera):
    """Official-parity camera extended with bounded MPEG-TS live."""

    _attr_has_entity_name = False
    _attr_supported_features = CameraEntityFeature.STREAM

    def __init__(self, runtime: BridgeRuntime, camera: dict[str, Any]) -> None:
        BlinkCameraEntity.__init__(self, runtime.coordinator, camera, "live-camera")
        Camera.__init__(self)
        self.runtime = runtime
        self._image: bytes | None = None
        self._thumbnail_url = camera.get("thumbnail_url")
        self._attr_name = f"{camera['name']} Live"
        self._attr_suggested_object_id = f"{camera['alias']}_live"
        self._attr_unique_id = f"{identity(camera)}-live-camera"
        self._attr_device_info = camera_device(camera)

    @property
    @override
    def extra_state_attributes(self) -> dict[str, Any]:
        return self.camera

    @property
    @override
    def motion_detection_enabled(self) -> bool:
        return bool(self.camera.get("enabled"))

    @override
    async def async_added_to_hass(self) -> None:
        await super().async_added_to_hass()
        await self._refresh_image()

    @callback
    @override
    def _handle_coordinator_update(self) -> None:
        if self.camera.get("thumbnail_url") != self._thumbnail_url:
            self.hass.async_create_task(self._refresh_image())
        super()._handle_coordinator_update()

    @override
    def camera_image(self, width: int | None = None, height: int | None = None) -> bytes | None:
        del width, height
        return self._image

    @override
    async def stream_source(self) -> str:
        return f"http://127.0.0.1:8123{API_PREFIX}/v1/cameras/{self.alias}/live.ts"

    @override
    async def async_enable_motion_detection(self) -> None:
        await self._command("motion", enabled=True)

    @override
    async def async_disable_motion_detection(self) -> None:
        await self._command("motion", enabled=False)

    async def record(self) -> None:
        await self._command("record")

    async def trigger_camera(self) -> None:
        await self._command("snapshot")
        await self._refresh_image()

    async def save_video(self, filename: str) -> None:
        self._validate_path(filename)
        clips = self._camera_clips()
        if not clips:
            raise HomeAssistantError("No Blink clip is available for this camera")
        content = await self.runtime.client.bytes(f"/v1/clips/{clips[0]['id']}")
        await self.hass.async_add_executor_job(Path(filename).write_bytes, content)

    async def save_recent_clips(self, file_path: str) -> None:
        self._validate_path(file_path)
        directory = Path(file_path)
        for index, clip in enumerate(self._camera_clips(), start=1):
            content = await self.runtime.client.bytes(f"/v1/clips/{clip['id']}")
            target = directory / f"{self.alias}-{index}.mp4"
            await self.hass.async_add_executor_job(target.write_bytes, content)

    async def _command(self, action: str, **values: Any) -> None:
        await self.runtime.client.post(
            f"/v1/cameras/{self.alias}/commands",
            {"action": action, **values},
        )
        await self.coordinator.async_request_refresh()

    async def _refresh_image(self) -> None:
        self._thumbnail_url = self.camera.get("thumbnail_url")
        try:
            self._image = await self.runtime.client.bytes(f"/v1/cameras/{self.alias}/snapshot.jpg")
        except EngineError:
            self._image = None

    def _camera_clips(self) -> list[dict[str, Any]]:
        clips = self.coordinator.data.get("clips", [])
        camera = self.camera
        return [
            clip
            for clip in clips
            if not clip.get("deleted")
            and (
                str(clip.get("camera_id")) == str(camera["id"])
                if clip.get("camera_id") is not None
                else clip["camera_name"] == camera["name"]
            )
        ]

    def _validate_path(self, path: str) -> None:
        if not self.hass.config.is_allowed_path(path):
            raise ServiceValidationError(f"Path is not allowed: {path}")
