"""Sharing video does not reserve the exclusive talk lease."""

import asyncio
from types import SimpleNamespace
from unittest.mock import AsyncMock

from test_walnut_websocket import EventUpstream, owned_session, relay  # noqa: F401


async def test_second_viewer_of_same_camera_gets_own_channel(relay, monkeypatch):  # noqa: F811
    hass, connection, first = owned_session(relay)
    first.owner = -1
    tasks = []
    hass.async_create_task = lambda task: tasks.append(asyncio.create_task(task)) or tasks[-1]
    client = SimpleNamespace(websocket=AsyncMock(return_value=EventUpstream([])))
    monkeypatch.setattr(relay, "_runtime", lambda _: SimpleNamespace(client=client))
    monkeypatch.setattr(relay, "uuid4", lambda: SimpleNamespace(hex="second"), raising=False)
    await relay.ws_start(hass, connection, {"id": 2, "alias": first.alias})
    connection.send_error.assert_not_called()
    client.websocket.assert_awaited_once()
    assert relay._sessions(hass)["second"].owner == id(connection)
    assert not first.closed
    await asyncio.gather(*tasks)
    assert relay._sessions(hass) == {"handle": first}
