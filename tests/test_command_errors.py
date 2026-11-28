"""A refused Blink command becomes a translated Home Assistant error, not "Unexpected exception"."""

import json
from contextlib import contextmanager
from pathlib import Path

import pytest
from test_device_health import COMPONENT, EngineError, load


class FakeHomeAssistantError(Exception):
    def __init__(self, translation_domain=None, translation_key=None, **_):
        super().__init__(translation_key)
        self.translation_domain = translation_domain
        self.translation_key = translation_key


MODULE = load(
    "command_errors.py",
    {
        "contextmanager": contextmanager,
        "HomeAssistantError": FakeHomeAssistantError,
        "EngineError": EngineError,
        "DOMAIN": "blink_live_bridge",
        "Iterator": object,
    },
    set(),
)
command_error, blink_command = MODULE["command_error"], MODULE["blink_command"]


@pytest.mark.parametrize(
    ("error", "key"),
    [
        (EngineError("conflict", 409), "blink_busy"),
        (EngineError("slow down", 429, None, 12), "blink_slow_down"),
        (EngineError("revoked", 403, "reauth_required"), "blink_signin"),
        (EngineError("forbidden", 403), "blink_unavailable"),
        (EngineError("bad gateway", 502), "blink_unavailable"),
        (EngineError("no answer"), "blink_unavailable"),
    ],
)
def test_each_failure_gets_its_own_message(error, key):
    result = command_error(error)
    assert (result.translation_domain, result.translation_key) == ("blink_live_bridge", key)


def test_the_wrapper_converts_engine_errors_and_nothing_else():
    with pytest.raises(FakeHomeAssistantError) as raised, blink_command():
        raise EngineError("conflict", 409)
    assert raised.value.translation_key == "blink_busy"
    assert isinstance(raised.value.__cause__, EngineError)
    with pytest.raises(ValueError, match="other"), blink_command():
        raise ValueError("other")
    with blink_command():
        pass  # Success is untouched.


def test_every_message_is_translated_in_both_languages():
    keys = {"blink_busy", "blink_slow_down", "blink_signin", "blink_unavailable"}
    for name in ("strings.json", "translations/en.json", "translations/it.json"):
        texts = json.loads((COMPONENT / name).read_text(encoding="utf-8"))["exceptions"]
        assert set(texts) == keys
        assert all(texts[key]["message"].strip() for key in keys)


def test_the_entity_actions_use_the_wrapper():
    for name, calls in (
        ("alarm_control_panel.py", 1),
        ("camera.py", 2),
        ("switch.py", 1),
    ):
        source = Path(COMPONENT / name).read_text(encoding="utf-8")
        assert source.count("with blink_command():") == calls, name
