"""Deterministic Blink IMMI framing tests without vendor credentials."""

import asyncio
import importlib.util
import sys
from enum import StrEnum
from itertools import pairwise
from pathlib import Path
from types import ModuleType

import pytest

COMPONENT = Path(__file__).parents[1] / "custom_components/blink_live_bridge"
TEST_PACKAGE = "vistoda_blink_protocol_test"


def _load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


@pytest.fixture
def protocol(monkeypatch: pytest.MonkeyPatch):
    """Load protocol.py behind minimal dependency stubs."""
    package = ModuleType(TEST_PACKAGE)
    package.__path__ = [str(COMPONENT)]
    monkeypatch.setitem(sys.modules, TEST_PACKAGE, package)

    homeassistant = ModuleType("homeassistant")
    homeassistant_const = ModuleType("homeassistant.const")

    class Platform(StrEnum):
        CAMERA = "camera"

    homeassistant_const.Platform = Platform
    monkeypatch.setitem(sys.modules, "homeassistant", homeassistant)
    monkeypatch.setitem(sys.modules, "homeassistant.const", homeassistant_const)

    blinkpy = ModuleType("blinkpy")
    blinkpy_livestream = ModuleType("blinkpy.livestream")
    blinkpy_livestream.BlinkLiveStream = object
    monkeypatch.setitem(sys.modules, "blinkpy", blinkpy)
    monkeypatch.setitem(sys.modules, "blinkpy.livestream", blinkpy_livestream)

    _load_module(f"{TEST_PACKAGE}.const", COMPONENT / "const.py")
    return _load_module(f"{TEST_PACKAGE}.protocol", COMPONENT / "protocol.py")


class FakeLive:
    """Minimal Blink live stream with controllable framed input."""

    def __init__(self) -> None:
        self.target_reader = asyncio.StreamReader()
        self.authenticated = False
        self.stopped = False

    async def auth(self) -> None:
        self.authenticated = True

    async def send(self) -> None:
        await asyncio.Event().wait()

    async def poll(self) -> None:
        await asyncio.Event().wait()

    def stop(self) -> None:
        self.stopped = True


def _header(message_type: int, payload_size: int) -> bytes:
    return bytes([message_type, 0, 0, 0, 0]) + payload_size.to_bytes(4, "big")


async def test_fragmented_header_and_payload_are_reassembled(protocol) -> None:
    """TCP fragmentation must not truncate IMMI packets."""
    live = FakeLive()
    payload = bytes([protocol.MPEG_TS_SYNC]) + b"bounded-mpeg-ts"
    frame = _header(protocol.VIDEO_MESSAGE, len(payload)) + payload
    offsets = (0, 2, 7, 11, len(frame))
    for start, end in pairwise(offsets):
        live.target_reader.feed_data(frame[start:end])
    live.target_reader.feed_eof()

    chunks = [chunk async for chunk in protocol.iter_mpegts(live)]

    assert chunks == [payload]
    assert live.authenticated
    assert live.stopped


async def test_oversized_payload_fails_before_allocation(protocol) -> None:
    """Reject unbounded vendor payload lengths before reading their body."""
    live = FakeLive()
    live.target_reader.feed_data(_header(protocol.VIDEO_MESSAGE, protocol.MAX_PACKET_BYTES + 1))
    live.target_reader.feed_eof()

    with pytest.raises(ValueError, match="IMMI payload exceeds"):
        _ = [chunk async for chunk in protocol.iter_mpegts(live)]

    assert live.stopped
