"""Blink arm/disarm programs and motion long-poll contracts (ADR 0014, ADR 0015)."""

import importlib.util
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import AsyncMock, Mock
from zoneinfo import ZoneInfo

from test_device_health import EngineError, UpdateFailedError, load

ROOT = Path(__file__).parents[1]
COMPONENT = ROOT / "custom_components/blink_live_bridge"
ENGINE = ROOT / "addon/vistoda_blink_engine/src"
ROME = ZoneInfo("Europe/Rome")
WEEKDAYS = ["mon", "tue", "wed", "thu", "fri"]


def schedule_module():
    path = COMPONENT / "program_schedule.py"
    spec = importlib.util.spec_from_file_location("program_schedule", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


SCHEDULE = schedule_module()


def action(kind: str, time: str, days: list[str]) -> dict:
    return {"action": kind, "time": time, "days": days, "device_count": 2}


def test_utc_schedule_becomes_local_weekly_moments() -> None:
    events = SCHEDULE.local_events(
        [
            action("arm", "2026-10-04 20:00:00 +0000", WEEKDAYS),
            action("disarm", "2026-10-04 05:30:00 +0000", WEEKDAYS),
        ],
        ROME,
    )
    assert events[:2] == [
        {"day": "mon", "time": "07:30", "action": "disarm"},
        {"day": "mon", "time": "22:00", "action": "arm"},
    ]
    assert len(events) == 10
    assert SCHEDULE.summary(events) == "lun-ven 07:30 disarma; lun-ven 22:00 arma"


def test_a_late_utc_sunday_is_a_local_monday_in_summer_and_winter() -> None:
    summer = SCHEDULE.local_events([action("arm", "2026-10-04 23:30:00 +0000", ["sun"])], ROME)
    winter = SCHEDULE.local_events([action("arm", "2026-12-06 23:30:00 +0000", ["sun"])], ROME)
    assert summer == [{"day": "mon", "time": "01:30", "action": "arm"}]
    assert winter == [{"day": "mon", "time": "00:30", "action": "arm"}]


def test_day_groups_collapse_only_runs_of_three() -> None:
    events = [
        {"day": day, "time": "09:00", "action": "disarm"} for day in ["sat", "sun", "mon", "wed"]
    ]
    events += [{"day": day, "time": "08:00", "action": "arm"} for day in ["wed", "thu", "fri"]]
    assert SCHEDULE.summary(events) == "lun, mer, sab, dom 09:00 disarma; mer-ven 08:00 arma"
    assert SCHEDULE.summary([]) == ""


def test_untrusted_schedules_are_ignored_and_bounded() -> None:
    junk = [
        "x",
        action("boom", "2026-10-04 20:00:00 +0000", ["mon"]),
        action("arm", "not a time", ["mon"]),
        action("arm", "2026-10-04 20:00:00 +0000", ["xyz", 5]),
        {"action": "arm", "time": "2026-10-04 20:00:00 +0000", "days": "mon"},
    ]
    assert SCHEDULE.local_events(junk, ROME) == []
    assert SCHEDULE.local_events(None, ROME) == []
    many = [action("arm", f"2026-10-04 {hour:02d}:00:00 +0000", WEEKDAYS) for hour in range(20)]
    assert len(SCHEDULE.local_events(many, ROME)) == SCHEDULE.MAX_EVENTS
    long = [
        {"day": "mon", "time": f"{minute // 60:02d}:{minute % 60:02d}", "action": "arm"}
        for minute in range(60)
    ]
    assert len(SCHEDULE.summary(long)) <= SCHEDULE.MAX_SUMMARY


def test_attributes_and_merge_expose_no_camera_ids() -> None:
    program = {
        "id": "4021",
        "network_id": "85507",
        "name": "Notte",
        "enabled": True,
        "status": "enabled",
        "schedule": [action("arm", "2026-10-04 20:00:00 +0000", ["mon"])],
    }
    attributes = SCHEDULE.program_attributes(program, ROME)
    assert attributes == {
        "program_id": "4021",
        "status": "enabled",
        "schedule": [{"day": "mon", "time": "22:00", "action": "arm"}],
        "summary": "lun 22:00 arma",
    }
    assert SCHEDULE.program_attributes(None, ROME)["summary"] is None
    other = {**program, "network_id": "1"}
    merged = SCHEDULE.merge_program([program, other, "x"], {**program, "enabled": False})
    assert [item["enabled"] for item in merged] == [True, False]
    assert merged[0]["network_id"] == "1"
    assert SCHEDULE.merge_program(None, {"no": "id"}) == []


def test_program_switches_are_config_entities_verified_by_the_engine() -> None:
    entity = (COMPONENT / "program.py").read_text(encoding="utf-8")
    switch = (COMPONENT / "switch.py").read_text(encoding="utf-8")
    assert "_attr_entity_category = EntityCategory.CONFIG" in entity
    assert "f\"program-{program['id']}\"" in entity, "unique ID: vistoda-{serial}-program-{id}"
    assert "f\"Programma: {program.get('name') or self.program_id}\"" in entity
    assert '/programs/{self.program_id}/enabled"' in entity
    assert "async_set_updated_data" in entity and "async_request_refresh" not in entity
    assert "MAX_PROGRAM_SWITCHES = 64" in entity
    assert "runtime.coordinator.async_add_listener(add_programs)" in switch
    routes = (ENGINE / "api_programs.rs").read_text(encoding="utf-8")
    assert '"/v1/networks/{network}/programs/{program}/enabled"' in routes
    assert '"/v1/programs"' in routes
    sync = (ENGINE / "blink_program_sync.rs").read_text(encoding="utf-8")
    assert "READ_BACK_ATTEMPTS" in sync and "SettingsVerification" in sync
    assert "PROGRAM_REFRESH" in sync


def motion_coordinator(responses: list) -> SimpleNamespace:
    namespace = load(
        "motion.py",
        {
            "EngineError": EngineError,
            "UpdateFailed": UpdateFailedError,
            "asyncio": SimpleNamespace(sleep=AsyncMock()),
            "time": SimpleNamespace(monotonic=Mock(side_effect=[0, 0, 0, 30, 0, 0, 0, 30])),
            "LONG_POLL_WAIT": 25,
            "LONG_POLL_TIMEOUT": 40,
            "LONG_POLL_RETRY": 15,
            "LONG_POLL_MIN_SECONDS": 1,
        },
        {"BlinkMotionCoordinator"},
    )
    coordinator = object.__new__(namespace["BlinkMotionCoordinator"])
    coordinator.supported = True
    coordinator.sequence = 3
    coordinator.client = SimpleNamespace(get_json=AsyncMock(side_effect=responses))
    coordinator.async_set_updated_data = Mock()
    coordinator.sleep = namespace["asyncio"].sleep
    return coordinator


async def test_long_poll_pushes_changes_and_stops_on_old_providers() -> None:
    camera = {"alias": "balcone", "event_id": "77", "thumbnail_available": True}
    coordinator = motion_coordinator(
        [
            {"sequence": 3, "cameras": []},  # unchanged and instant: back off
            {"sequence": 4, "cameras": [camera]},
            EngineError("down", 502),
            {"cameras": []},  # no sequence: an older provider, stop
        ]
    )
    await coordinator.async_long_poll()
    paths = [call.args[0] for call in coordinator.client.get_json.call_args_list]
    assert paths == [
        "/v1/motion?since=3&wait=25",
        "/v1/motion?since=3&wait=25",
        "/v1/motion?since=4&wait=25",
        "/v1/motion?since=4&wait=25",
    ]
    coordinator.async_set_updated_data.assert_called_once_with({"balcone": camera})
    assert coordinator.sleep.await_count == 2
    assert coordinator.sequence is None
