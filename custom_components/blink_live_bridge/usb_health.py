"""Pure mapping from Blink Sync Module USB status to Home Assistant states.

Kept free of Home Assistant imports so the mapping is unit-tested directly.
The source values are the engine's ``LocalStorageStatus`` fields; the native
``usb_state`` values are Blink Android's ``LocalStorageState`` (ADR 0011).
"""

from typing import Any

# Sensor states. Blink's ``unavailable`` is published as ``removed`` because
# ``unavailable`` is reserved by Home Assistant for unreachable entities.
USB_STATES = (
    "ok",
    "almost_full",
    "full",
    "format_required",
    "removed",
    "unmounted",
    "incompatible",
)
NATIVE_STATES = {
    "active": "ok",
    "memory_full": "full",
    "format_required": "format_required",
    "unavailable": "removed",
    "unmounted": "unmounted",
    "incompatible": "incompatible",
}
HEALTHY_STATES = frozenset({"ok", "almost_full"})
# The official app shows its "almost full" banner from storage_warning >= 3.
ALMOST_FULL_WARNING = 3
BACKUP_FAILURE_WORDS = ("fail", "error")


def usb_state(status: Any) -> str | None:
    """Return the normalized state, or None when Blink gives no usable USB info."""
    if not isinstance(status, dict):
        return None
    raw = status.get("usb_state")
    if not isinstance(raw, str):
        return None
    state = NATIVE_STATES.get(raw.strip().lower())
    if state != "ok":
        # Unknown values stay unknown: the raw value remains an attribute.
        return state
    if status.get("usb_storage_full") is True:
        return "full"
    warning = status.get("storage_warning")
    if isinstance(warning, int) and warning >= ALMOST_FULL_WARNING:
        return "almost_full"
    return "ok"


def backup_failed(status: Any) -> bool:
    """True when Blink reports that the last cloud-to-USB backup failed."""
    result = status.get("last_backup_result") if isinstance(status, dict) else None
    return isinstance(result, str) and any(word in result.lower() for word in BACKUP_FAILURE_WORDS)


def usb_problem(status: Any) -> bool | None:
    """Whether the drive needs attention; None when the state is unknown."""
    if backup_failed(status):
        return True
    state = usb_state(status)
    return None if state is None else state not in HEALTHY_STATES


def usb_attributes(status: Any) -> dict[str, Any]:
    """Expose the provider facts behind the normalized state."""
    status = status if isinstance(status, dict) else {}
    return {
        "usb_state": status.get("usb_state") or None,
        "percent_used": status.get("usb_storage_used"),
        "storage_warning": status.get("storage_warning"),
        "storage_full": status.get("usb_storage_full"),
        "backup_enabled": status.get("backup_enabled"),
        "backup_in_progress": status.get("backup_in_progress"),
        "backup_failed": backup_failed(status),
        "last_backup_completed": status.get("last_backup_completed"),
        "last_backup_result": status.get("last_backup_result"),
    }
