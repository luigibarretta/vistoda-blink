"""The adapter's parent device is found without the deprecated registry lookups."""

from types import SimpleNamespace
from unittest.mock import Mock

from test_live_backoff import functions

# --- Device registry --------------------------------------------------------------------------

LEGACY, PARENT = ("media_bridge", "blink:blink"), ("blink_live_bridge", "blink:blink")
INIT = functions(
    "__init__.py",
    {"dr": SimpleNamespace(), "LEGACY_PARENT_IDENTIFIER": LEGACY, "PARENT_IDENTIFIER": PARENT},
)
registered = INIT["_registered_device"]


class NewRegistry:
    """Home Assistant 2026.10+: one config entry per device, lookup needs the entry."""

    def __init__(self, devices):
        self.devices = devices  # {(identifier, entry_id): device}

    def async_get_device_by_identifier(self, identifier, config_entry_id):
        return self.devices.get((identifier, config_entry_id))

    def async_get_device(self, **_):
        raise AssertionError("the deprecated lookup must not be used")


class OldRegistry:
    """Before that split: devices may be shared, lookup is by identifier alone."""

    def __init__(self, device):
        self.device = device

    def async_get_device(self, identifiers):
        return (
            self.device
            if self.device and next(iter(identifiers)) in self.device.identifiers
            else None
        )


def device(entries, identifier=LEGACY):
    return SimpleNamespace(identifiers={identifier}, config_entries=set(entries))


def test_new_home_assistant_looks_a_device_up_by_identifier_and_entry():
    ours = device({"e1"})
    registry = NewRegistry({(LEGACY, "e1"): ours})
    assert registered(registry, LEGACY, "e1") is ours
    assert registered(registry, LEGACY, "other-entry") is None
    assert registered(registry, PARENT, "e1") is None


def test_older_home_assistant_still_needs_the_device_to_belong_to_this_entry_alone():
    assert registered(OldRegistry(device({"e1"})), LEGACY, "e1") is not None
    assert registered(OldRegistry(device({"e1", "e2"})), LEGACY, "e1") is None
    assert registered(OldRegistry(device({"e2"})), LEGACY, "e1") is None
    assert registered(OldRegistry(None), LEGACY, "e1") is None


def parent(registry, entry_id="e1"):
    registry.async_update_device = Mock()
    registry.async_get_or_create = Mock(return_value="parent")
    INIT["dr"].async_get = lambda hass: registry
    result = INIT["_parent_device"](object(), SimpleNamespace(entry_id=entry_id))
    return result, registry


def test_the_legacy_device_of_this_entry_is_migrated_once():
    legacy = SimpleNamespace(id="d1", identifiers={LEGACY, ("x", "y")}, config_entries={"e1"})
    result, registry = parent(NewRegistry({(LEGACY, "e1"): legacy}))
    assert result == "parent"
    registry.async_update_device.assert_called_once_with("d1", new_identifiers={("x", "y"), PARENT})
    # Already migrated (the new identifier exists for this entry): nothing is touched.
    done = NewRegistry({(LEGACY, "e1"): legacy, (PARENT, "e1"): device({"e1"}, PARENT)})
    _, registry = parent(done)
    registry.async_update_device.assert_not_called()
    # Nothing legacy at all: just the parent device.
    _, registry = parent(NewRegistry({}))
    registry.async_update_device.assert_not_called()
    registry.async_get_or_create.assert_called_once()
