"""Constants for Blink Live Bridge."""

from homeassistant.const import Platform

DOMAIN = "blink_live_bridge"
BLINK_DOMAIN = "blink"
VISTODA_DOMAIN = "media_bridge"
VISTODA_BLINK_IDENTIFIER = "blink:blink"
CONF_TOKEN = "token"
API_PREFIX = f"/api/{DOMAIN}"
PLATFORMS = (Platform.CAMERA,)

MAX_PACKET_BYTES = 4 * 1024 * 1024
QUEUE_DEPTH = 12
BATTERY_SESSION_SECONDS = 75
POWERED_SESSION_SECONDS = 600
POWERED_CAMERA_TYPES = frozenset({"owl", "mini"})
