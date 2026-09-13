"""Versioned, redacted backups for Blink camera settings."""

import asyncio
from contextlib import suppress
from copy import deepcopy
from datetime import UTC, datetime
from typing import Any
from uuid import uuid4

from homeassistant.core import HomeAssistant
from homeassistant.helpers.storage import Store

from .client import EngineError
from .const import DOMAIN
from .runtime import BridgeRuntime
from .settings_backup_model import (
    backup_for_camera,
    filter_backup_cameras,
    match_camera,
    select_cameras,
    summary,
    valid_backup,
)
from .settings_restore import RESTORE_ERRORS, CameraApplyError, apply_camera, restore_plan

STORE_VERSION = 1
STORE_KEY = f"{DOMAIN}.settings_backups"
MAX_BACKUPS = 25


class SettingsBackupManager:
    """Persist bounded snapshots and restore them through the typed engine API."""

    def __init__(self, hass: HomeAssistant) -> None:
        self.store = Store(hass, STORE_VERSION, STORE_KEY)
        self.lock = asyncio.Lock()

    async def summaries(self) -> list[dict[str, Any]]:
        async with self.lock:
            return [summary(item) for item in await self._load()]

    async def create(self, runtime: BridgeRuntime, name: str, aliases: list[str] | None) -> dict:
        async with self.lock:
            backup = await self._capture(runtime, name, aliases, "manual")
            items = await self._load()
            items.insert(0, backup)
            await self.store.async_save({"backups": items[:MAX_BACKUPS]})
            return summary(backup)

    async def delete(self, backup_id: str) -> bool:
        async with self.lock:
            items = await self._load()
            kept = [item for item in items if item.get("backup_id") != backup_id]
            if len(kept) == len(items):
                return False
            await self.store.async_save({"backups": kept})
            return True

    async def apply(
        self, runtime: BridgeRuntime, backup_id: str, aliases: list[str] | None
    ) -> dict:
        async with self.lock:
            items = await self._load()
            backup = next((item for item in items if item.get("backup_id") == backup_id), None)
            if backup is None:
                raise LookupError
            targets = filter_backup_cameras(backup["cameras"], aliases)
            resolved = self._resolve(runtime, targets)
            rollback = await self._capture(
                runtime,
                f"Rollback · {backup['name']}",
                [camera["alias"] for _, camera in resolved],
                "rollback",
            )
            plans = self._preflight(resolved, rollback)
            items.insert(0, rollback)
            await self.store.async_save({"backups": items[:MAX_BACKUPS]})
            completed: list[tuple[dict, dict, str]] = []
            current_failure: tuple[dict, dict, CameraApplyError] | None = None
            try:
                for target, camera, previous in plans:
                    try:
                        changed, revision = await apply_camera(
                            runtime, camera["alias"], target["settings"], previous["revision"]
                        )
                    except CameraApplyError as error:
                        current_failure = (camera, previous, error)
                        raise
                    if changed:
                        completed.append((camera, previous, revision))
            except RESTORE_ERRORS as error:
                await self._rollback(runtime, completed, current_failure)
                await _refresh_runtime(runtime)
                if isinstance(error, EngineError):
                    raise
                raise EngineError("Blink settings restore failed") from error
            await _refresh_runtime(runtime)
            return {
                "applied_cameras": len(resolved),
                "rollback_backup_id": rollback["backup_id"],
            }

    @staticmethod
    def _resolve(runtime: BridgeRuntime, targets: list[dict]) -> list[tuple[dict, dict]]:
        resolved = []
        aliases: set[str] = set()
        for target in targets:
            camera = match_camera(runtime.cameras, target)
            if camera is None:
                raise EngineError("backup camera is no longer available")
            if camera["alias"] in aliases:
                raise EngineError("backup resolves more than once to the same camera")
            aliases.add(camera["alias"])
            resolved.append((target, camera))
        return resolved

    @staticmethod
    def _preflight(
        resolved: list[tuple[dict, dict]], rollback: dict
    ) -> list[tuple[dict, dict, dict]]:
        plans = []
        for target, camera in resolved:
            previous = backup_for_camera(rollback["cameras"], camera)
            if previous is None:
                raise EngineError("rollback snapshot identity mismatch")
            restore_plan(previous["settings"], target["settings"])
            plans.append((target, camera, previous))
        return plans

    @staticmethod
    async def _rollback(
        runtime: BridgeRuntime,
        completed: list[tuple[dict, dict, str]],
        current_failure: tuple[dict, dict, CameraApplyError] | None,
    ) -> None:
        targets = list(reversed(completed))
        if current_failure and current_failure[2].changed and current_failure[2].status != 409:
            camera, previous, failure = current_failure
            if failure.revision:
                targets.insert(0, (camera, previous, failure.revision))
        for camera, previous, expected_revision in targets:
            with suppress(*RESTORE_ERRORS):
                await apply_camera(
                    runtime, camera["alias"], previous["settings"], expected_revision
                )

    async def _capture(
        self, runtime: BridgeRuntime, name: str, aliases: list[str] | None, reason: str
    ) -> dict:
        cameras = []
        for camera in select_cameras(runtime.cameras, aliases):
            settings = await runtime.client.get_json(f"/v1/cameras/{camera['alias']}/settings")
            revision = settings.get("revision")
            fields = settings.get("settings")
            camera_id = camera.get("id")
            if not isinstance(revision, str) or not revision or not isinstance(fields, list):
                raise EngineError("provider returned an incomplete settings document")
            if camera_id in (None, "") and not camera.get("serial"):
                raise EngineError("Blink camera has no stable provider identity")
            if any(
                isinstance(field, dict)
                and field.get("writable") is True
                and (not isinstance(field.get("key"), str) or "value" not in field)
                for field in fields
            ):
                raise EngineError("provider returned a malformed writable setting")
            captured = {
                "camera_id": str(camera_id) if camera_id not in (None, "") else None,
                "serial": camera.get("serial"),
                "network_id": str(camera["network_id"]),
                "alias": camera["alias"],
                "name": settings.get("name") or camera.get("name") or camera["alias"],
                "revision": revision,
                "settings": [
                    deepcopy(field)
                    for field in fields
                    if isinstance(field, dict)
                    and field.get("writable") is True
                    and isinstance(field.get("key"), str)
                    and "value" in field
                ],
            }
            if not captured["settings"]:
                raise EngineError("provider returned no writable camera settings")
            cameras.append(captured)
        if not cameras:
            raise EngineError("no Blink cameras selected")
        return {
            "backup_id": uuid4().hex,
            "name": name.strip()[:64] or "Blink settings",
            "created_at": datetime.now(UTC).isoformat(),
            "reason": reason,
            "cameras": cameras,
        }

    async def _load(self) -> list[dict[str, Any]]:
        data = await self.store.async_load() or {}
        items = data.get("backups", []) if isinstance(data, dict) else []
        return [item for item in items if valid_backup(item)] if isinstance(items, list) else []


async def _refresh_runtime(runtime: BridgeRuntime) -> None:
    with suppress(EngineError):
        state = await runtime.client.get_json("/v1/state")
        runtime.coordinator.async_set_updated_data(state)
