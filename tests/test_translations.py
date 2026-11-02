"""Custom integrations read their texts only from translations/<language>.json."""

import json
from pathlib import Path

COMPONENT = Path(__file__).parents[1] / "custom_components" / "blink_live_bridge"


def keys(value, prefix=""):
    if not isinstance(value, dict):
        return {prefix}
    return {key for name, item in value.items() for key in keys(item, f"{prefix}/{name}")}


def load(name):
    return json.loads((COMPONENT / name).read_text(encoding="utf-8"))


def test_english_is_shipped_as_the_fallback():
    # strings.json is never read for custom integrations; en.json is the fallback.
    assert load("translations/en.json") == load("strings.json")


def test_every_translation_covers_the_same_texts():
    assert keys(load("translations/it.json")) == keys(load("strings.json"))
