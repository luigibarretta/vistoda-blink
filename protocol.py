"""Robust IMMI framing for Blink MPEG-TS live streams."""

import asyncio
from collections.abc import AsyncIterator
import contextlib
import logging

from blinkpy.livestream import BlinkLiveStream

from .const import MAX_PACKET_BYTES

_LOGGER = logging.getLogger(__name__)
HEADER_BYTES = 9
VIDEO_MESSAGE = 0x00
MPEG_TS_SYNC = 0x47


async def iter_mpegts(live: BlinkLiveStream) -> AsyncIterator[bytes]:
    """Yield complete MPEG-TS payloads and always close the Blink command."""
    await live.auth()
    sender = asyncio.create_task(live.send(), name="blink-live-keepalive")
    poller = asyncio.create_task(live.poll(), name="blink-live-command-poll")
    try:
        while not live.target_reader.at_eof():
            header = await live.target_reader.readexactly(HEADER_BYTES)
            message_type = header[0]
            payload_length = int.from_bytes(header[5:9], byteorder="big")
            if payload_length <= 0:
                continue
            if payload_length > MAX_PACKET_BYTES:
                raise ValueError(f"IMMI payload exceeds {MAX_PACKET_BYTES} bytes")
            payload = await live.target_reader.readexactly(payload_length)
            if message_type == VIDEO_MESSAGE and payload[0] == MPEG_TS_SYNC:
                yield payload
    except asyncio.IncompleteReadError:
        _LOGGER.debug("Blink closed the live stream")
    finally:
        live.stop()
        for task in (sender, poller):
            if not task.done():
                task.cancel()
        with contextlib.suppress(Exception, asyncio.CancelledError):
            await asyncio.gather(sender, poller)
