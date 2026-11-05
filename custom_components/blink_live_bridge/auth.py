"""Private HTTP authentication for Blink Live Bridge."""

import base64
import hmac

from aiohttp import web
from homeassistant.components.http import KEY_AUTHENTICATED

# Home Assistant redacts this query parameter when it logs a stream source.
STREAM_TOKEN_PARAM = "auth"


def _presented_token(request: web.Request) -> str:
    scheme, _, value = request.headers.get("Authorization", "").partition(" ")
    if scheme.casefold() == "bearer":
        return value
    if scheme.casefold() != "basic":
        return ""
    try:
        encoded = base64.b64decode(value, validate=True).decode()
    except (ValueError, UnicodeDecodeError):
        return ""
    _, separator, password = encoded.partition(":")
    return password if separator else ""


def _matches(presented: str, expected: str | None) -> bool:
    # Bytes: compare_digest raises TypeError on non-ASCII str input.
    return bool(presented and expected) and hmac.compare_digest(
        presented.encode(), expected.encode()
    )


def require_bridge_auth(
    request: web.Request,
    expected_token: str,
    stream_token: str | None = None,
) -> None:
    """Require Home Assistant authentication or a constant-time token match.

    The TCP peer is never trusted: behind a reverse proxy or tunnel every
    remote request arrives from loopback. ``stream_token`` is the per-camera
    secret embedded in that camera's own ``stream_source`` URL.
    """
    if request.get(KEY_AUTHENTICATED):
        return
    if _matches(_presented_token(request), expected_token):
        return
    if _matches(request.query.get(STREAM_TOKEN_PARAM, ""), stream_token):
        return
    raise web.HTTPUnauthorized(headers={"WWW-Authenticate": 'Bearer realm="blink"'})
