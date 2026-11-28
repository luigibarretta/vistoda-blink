"""Turn a refused Blink command into a message the user can act on.

Without this, a rejected "arm" or "snapshot" reaches Home Assistant as an unhandled
`EngineError` and the user sees "Unexpected exception".
"""

from collections.abc import Iterator
from contextlib import contextmanager

from homeassistant.exceptions import HomeAssistantError

from .client import EngineError
from .const import DOMAIN


def command_error(error: EngineError) -> HomeAssistantError:
    """The translated error for a failed engine call (texts in translations/*.json)."""
    if error.status == 409:
        key = "blink_busy"  # Blink runs one command per system: e.g. a live view is open.
    elif error.status == 429:
        key = "blink_slow_down"
    elif error.reauth_required:
        key = "blink_signin"
    else:
        key = "blink_unavailable"
    return HomeAssistantError(translation_domain=DOMAIN, translation_key=key)


@contextmanager
def blink_command() -> Iterator[None]:
    """Wrap a call to the engine made on behalf of a service or an entity action."""
    try:
        yield
    except EngineError as error:
        raise command_error(error) from error
