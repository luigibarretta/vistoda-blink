"""Stable public constants for Vistoda Blink."""

from homeassistant.const import Platform

DOMAIN = "blink_live_bridge"
PARENT_IDENTIFIER = (DOMAIN, "adapter")
# Releases up to 0.24.3 registered the adapter device under Vistoda's domain;
# matched only to move this entry's own device to PARENT_IDENTIFIER.
LEGACY_PARENT_IDENTIFIER = ("media_bridge", "blink:blink")
CONF_TOKEN = "token"
CONF_URL = "url"
CONF_MANAGED_APP = "managed_app"
CONF_SCAN_INTERVAL = "scan_interval"
DEFAULT_SCAN_INTERVAL = 300
ENGINE_URL = "http://local-vistoda-blink-engine:8099"
API_PREFIX = f"/api/{DOMAIN}"
PLATFORMS = (
    Platform.ALARM_CONTROL_PANEL,
    Platform.BINARY_SENSOR,
    Platform.CAMERA,
    Platform.SENSOR,
    Platform.SWITCH,
)
