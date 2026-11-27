"""A refused live view says when to come back (HTTP 429 + Retry-After)."""

import aiohttp
import pytest
from aiohttp import web
from test_device_health import (  # noqa: F401  (AST loader and the client module)
    COMPONENT,
    EngineError,
    load,
)


def functions(name: str, namespace: dict, strip: set[str] | None = None) -> dict:
    return load(name, namespace, strip or set())


# --- Retry-After ------------------------------------------------------------------------------

CLIENT = load(
    "client.py",
    {"ClientError": aiohttp.ClientError, "ClientTimeout": aiohttp.ClientTimeout, "ENGINE_URL": ""},
    set(),
)


@pytest.mark.parametrize(
    ("header", "seconds"),
    [("30", 30), ("0", 0), ("9999", 300), (None, None), ("", None), ("soon", None), ("-5", None)],
)
def test_retry_after_is_whole_seconds_and_bounded(header, seconds):
    assert CLIENT["_retry_after"](header) == seconds


def test_engine_error_remembers_the_wait():
    error = EngineError("provider returned HTTP 429", 429, None, 12)
    assert (error.status, error.retry_after) == (429, 12)
    assert EngineError("x").retry_after is None


HTTP = functions(
    "http.py",
    {
        "web": web,
        "EngineError": EngineError,
        "API_PREFIX": "",
        "DOMAIN": "blink_live_bridge",
        "HomeAssistantView": object,
    },
)


def test_a_refused_live_view_tells_home_assistant_when_to_retry():
    refused = HTTP["live_unavailable"](EngineError("busy", 429, None, 17))
    assert isinstance(refused, web.HTTPTooManyRequests)
    assert refused.headers["Retry-After"] == "17"
    # No wait given: a short default rather than none at all.
    assert HTTP["live_unavailable"](EngineError("busy", 429)).headers["Retry-After"] == "30"


@pytest.mark.parametrize("status", [None, 400, 409, 500, 502])
def test_any_other_live_failure_stays_a_bad_gateway(status):
    failure = HTTP["live_unavailable"](EngineError("down", status))
    assert isinstance(failure, web.HTTPBadGateway)
    assert "Retry-After" not in failure.headers
