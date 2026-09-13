"""Validation and stable identity matching for Blink settings backups."""

from .client import EngineError


def valid_backup(item: object) -> bool:
    if not isinstance(item, dict) or not all(
        isinstance(item.get(key), str) for key in ("backup_id", "name", "created_at", "reason")
    ):
        return False
    cameras = item.get("cameras")
    return (
        len(item["backup_id"]) == 32
        and all(character in "0123456789abcdef" for character in item["backup_id"])
        and bool(item["name"].strip())
        and isinstance(cameras, list)
        and bool(cameras)
        and all(
            isinstance(camera, dict)
            and isinstance(camera.get("alias"), str)
            and bool(camera["alias"])
            and isinstance(camera.get("revision"), str)
            and bool(camera["revision"])
            and bool(camera.get("serial") or camera.get("camera_id"))
            and isinstance(camera.get("settings"), list)
            and bool(camera["settings"])
            and all(
                isinstance(field, dict)
                and isinstance(field.get("key"), str)
                and field.get("writable") is True
                and "value" in field
                for field in camera.get("settings", [])
            )
            for camera in cameras
        )
    )


def summary(item: dict) -> dict:
    return {key: item[key] for key in ("backup_id", "name", "created_at", "reason")} | {
        "camera_count": len(item.get("cameras", [])),
        "cameras": [
            {key: camera.get(key) for key in ("camera_id", "serial", "alias", "name")}
            for camera in item.get("cameras", [])
        ],
    }


def select_cameras(cameras: list[dict], aliases: list[str] | None) -> list[dict]:
    if aliases is None:
        return list(cameras)
    requested = set(aliases)
    selected = [camera for camera in cameras if camera.get("alias") in requested]
    if len(selected) != len(requested):
        raise EngineError("unknown Blink camera alias")
    return selected


def filter_backup_cameras(cameras: list[dict], aliases: list[str] | None) -> list[dict]:
    if aliases is None:
        return cameras
    requested = set(aliases)
    selected = [camera for camera in cameras if camera.get("alias") in requested]
    if len(selected) != len(requested):
        raise EngineError("camera is absent from backup")
    return selected


def match_camera(cameras: list[dict], saved: dict) -> dict | None:
    serial = saved.get("serial")
    if serial:
        matches = [camera for camera in cameras if str(camera.get("serial") or "") == str(serial)]
        if len(matches) == 1:
            return matches[0]
        if any(camera.get("serial") for camera in cameras):
            return None
    camera_id = saved.get("camera_id") or saved.get("id")
    if camera_id:
        matches = [camera for camera in cameras if str(camera.get("id") or "") == str(camera_id)]
        return matches[0] if len(matches) == 1 else None
    matches = [camera for camera in cameras if camera.get("alias") == saved.get("alias")]
    return matches[0] if len(matches) == 1 else None


def backup_for_camera(saved_cameras: list[dict], camera: dict) -> dict | None:
    serial = camera.get("serial")
    if serial:
        matches = [saved for saved in saved_cameras if saved.get("serial") == serial]
        if len(matches) == 1:
            return matches[0]
    camera_id = str(camera.get("id") or "")
    matches = [saved for saved in saved_cameras if str(saved.get("camera_id") or "") == camera_id]
    return matches[0] if camera_id and len(matches) == 1 else None
