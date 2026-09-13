"""Optimistic, fail-closed restoration of Blink camera settings."""

from typing import Any

from .client import EngineError
from .runtime import BridgeRuntime

RESTORE_ERRORS = (EngineError, KeyError, TypeError, ValueError)


class CameraApplyError(EngineError):
    """A failed restore with the last provider revision Vistoda confirmed."""

    def __init__(self, error: Exception, changed: bool, revision: str | None) -> None:
        super().__init__("Blink camera settings restore failed", getattr(error, "status", None))
        self.changed = changed
        self.revision = revision


async def apply_camera(
    runtime: BridgeRuntime,
    alias: str,
    saved_fields: list[dict],
    expected_revision: str,
) -> tuple[bool, str]:
    changed = False
    revision: str | None = expected_revision
    try:
        current = await runtime.client.get_json(f"/v1/cameras/{alias}/settings")
        if current.get("revision") != expected_revision:
            raise EngineError("Blink settings changed after backup preflight", 409)
        plan = restore_plan(current.get("settings", []), saved_fields)
        for key, value in plan:
            current = await runtime.client.post(
                f"/v1/cameras/{alias}/settings",
                {"key": key, "value": value, "revision": current["revision"]},
            )
            changed = True
            revision = current.get("revision")
            if not isinstance(revision, str):
                raise EngineError("provider omitted the settings revision")
        verified = await runtime.client.get_json(f"/v1/cameras/{alias}/settings")
        if verified.get("revision") != revision or not values_match(
            verified.get("settings", []), saved_fields
        ):
            raise EngineError("Blink settings changed during restore verification", 409)
        return changed, revision
    except RESTORE_ERRORS as error:
        raise CameraApplyError(error, changed, revision) from error


def restore_plan(current_fields: list[dict], saved_fields: list[dict]) -> list[tuple[str, Any]]:
    current = {
        field.get("key"): field
        for field in current_fields
        if isinstance(field, dict) and isinstance(field.get("key"), str)
    }
    desired = {
        field["key"]: field.get("value")
        for field in saved_fields
        if isinstance(field, dict) and isinstance(field.get("key"), str)
    }
    for key, value in desired.items():
        field = current.get(key)
        if not field or field.get("writable") is not True:
            raise EngineError("a backed-up setting is no longer writable")
        old = field.get("value")
        if (
            key in {"temperature_min", "temperature_max"}
            and old != value
            and (old is None or value is None)
        ):
            raise EngineError("uninitialized temperature thresholds cannot be restored", 422)
    if desired.get("temperature_alerts") is True and not all(
        isinstance(desired.get(key), int) for key in ("temperature_min", "temperature_max")
    ):
        raise EngineError("temperature alerts require initialized thresholds", 422)
    changed = {key: value for key, value in desired.items() if current[key].get("value") != value}
    document = {"settings": list(current.values())}
    return ordered_values(document, changed)


def values_match(current_fields: list[dict], saved_fields: list[dict]) -> bool:
    current = {field.get("key"): field.get("value") for field in current_fields}
    return all(
        field.get("key") in current and current[field.get("key")] == field.get("value")
        for field in saved_fields
    )


def ordered_values(current: dict, desired: dict) -> list[tuple[str, Any]]:
    original = {field.get("key"): field.get("value") for field in current.get("settings", [])}

    def rank(item: tuple[str, Any]) -> int:
        key, value = item
        if key == "temperature_alerts":
            return 3 if value else -1
        if key == "temperature_min":
            return 0 if value < original[key] else 2
        if key == "temperature_max":
            return 0 if value > original[key] else 2
        return 1

    return sorted(desired.items(), key=rank)
