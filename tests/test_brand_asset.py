"""Keep the custom integration's local brand assets at Home Assistant's sizes."""

import struct
from pathlib import Path

import pytest

BRAND = Path(__file__).resolve().parents[1] / "custom_components/blink_live_bridge/brand"


@pytest.mark.parametrize(("name", "size"), [("icon.png", 256), ("icon@2x.png", 512)])
def test_local_brand_is_a_real_square_png(name: str, size: int) -> None:
    image = (BRAND / name).read_bytes()
    assert image[:8] == b"\x89PNG\r\n\x1a\n"
    assert image[12:16] == b"IHDR"
    width, height = struct.unpack(">II", image[16:24])
    assert width == height == size
    assert image[-8:-4] == b"IEND"
