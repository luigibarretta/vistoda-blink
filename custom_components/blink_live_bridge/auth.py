"""Private HTTP authentication for Blink Live Bridge."""

import base64
import hmac

from aiohttp import web


def _peer_is_loopback(request: web.Request) -> bool:
    peer = request.transport.get_extra_info("peername") if request.transport else None
    return bool(peer and peer[0] in {"127.0.0.1", "::1"})


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


def require_bridge_auth(request: web.Request, expected_token: str) -> None:
    """Allow Core loopback or a constant-time Bearer/Basic token match."""
    if _peer_is_loopback(request):
        return
    if not hmac.compare_digest(_presented_token(request), expected_token):
        raise web.HTTPUnauthorized(headers={"WWW-Authenticate": 'Bearer realm="blink"'})
