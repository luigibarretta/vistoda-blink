"""Pure summary of Blink arm/disarm program schedules (ADR 0014).

Blink stores each action's time as ``yyyy-MM-dd HH:mm:ss Z`` (UTC) and its
weekdays as UTC abbreviations. Like the official app, the date anchors each
weekday inside its Monday-based week before conversion to the local zone.
Kept free of Home Assistant imports so it is unit-tested directly.
"""

from datetime import datetime, timedelta, tzinfo
from typing import Any

DAYS = ("mon", "tue", "wed", "thu", "fri", "sat", "sun")
DAY_LABELS = ("lun", "mar", "mer", "gio", "ven", "sab", "dom")
ACTION_LABELS = {"arm": "arma", "disarm": "disarma"}
TIME_FORMAT = "%Y-%m-%d %H:%M:%S %z"
MAX_ACTIONS = 32
MAX_EVENTS = 64
MAX_SUMMARY = 240


def local_events(schedule: Any, zone: tzinfo) -> list[dict[str, str]]:
    """Weekly arm/disarm moments in ``zone``, sorted from Monday."""
    if not isinstance(schedule, list):
        return []
    events = []
    for action in schedule[:MAX_ACTIONS]:
        if not isinstance(action, dict) or action.get("action") not in ACTION_LABELS:
            continue
        moment = _parse(action.get("time"))
        days = action.get("days")
        if moment is None or not isinstance(days, list):
            continue
        for day in days:
            if day not in DAYS:
                continue
            anchored = moment + timedelta(days=DAYS.index(day) - moment.weekday())
            local = anchored.astimezone(zone)
            events.append(
                {
                    "day": DAYS[local.weekday()],
                    "time": local.strftime("%H:%M"),
                    "action": action["action"],
                }
            )
    events.sort(key=lambda event: (DAYS.index(event["day"]), event["time"], event["action"]))
    return events[:MAX_EVENTS]


def summary(events: list[dict[str, str]]) -> str:
    """Group equal times, e.g. ``lun-ven 22:00 arma; sab, dom 09:00 disarma``."""
    groups: dict[tuple[str, str], list[int]] = {}
    for event in events:
        key = (event["time"], event["action"])
        groups.setdefault(key, []).append(DAYS.index(event["day"]))
    ordered = sorted(groups.items(), key=lambda item: (min(item[1]), item[0]))
    text = "; ".join(
        f"{_days(days)} {time} {ACTION_LABELS[action]}" for (time, action), days in ordered
    )
    return text if len(text) <= MAX_SUMMARY else f"{text[: MAX_SUMMARY - 1]}…"


def program_attributes(program: dict[str, Any] | None, zone: tzinfo) -> dict[str, Any]:
    """State attributes of a program switch; no camera IDs are exposed."""
    program = program if isinstance(program, dict) else {}
    events = local_events(program.get("schedule"), zone)
    return {
        "program_id": program.get("id"),
        "status": program.get("status"),
        "schedule": events,
        "summary": summary(events) or None,
    }


def merge_program(programs: Any, program: Any) -> list[dict[str, Any]]:
    """Replace one program in the cached list with the engine's verified copy."""
    current = (
        [item for item in programs if isinstance(item, dict)] if isinstance(programs, list) else []
    )
    if not isinstance(program, dict) or "id" not in program:
        return current
    key = (str(program["id"]), str(program.get("network_id")))
    kept = [item for item in current if (str(item.get("id")), str(item.get("network_id"))) != key]
    return [*kept, program]


def _parse(value: Any) -> datetime | None:
    if not isinstance(value, str) or len(value) > 40:
        return None
    try:
        return datetime.strptime(value, TIME_FORMAT)
    except ValueError:
        return None


def _days(indexes: list[int]) -> str:
    """Collapse runs of three or more days into ``lun-ven``."""
    values = sorted(set(indexes))
    parts: list[str] = []
    start = previous = values[0]
    for value in [*values[1:], None]:
        if value is not None and value == previous + 1:
            previous = value
            continue
        if previous - start >= 2:
            parts.append(f"{DAY_LABELS[start]}-{DAY_LABELS[previous]}")
        else:
            parts.extend(DAY_LABELS[index] for index in range(start, previous + 1))
        if value is not None:
            start = previous = value
    return ", ".join(parts)
