"""Official Blink service parity for Vistoda camera entities."""

import voluptuous as vol
from homeassistant.components.camera import DOMAIN as CAMERA_DOMAIN
from homeassistant.const import CONF_FILE_PATH, CONF_FILENAME
from homeassistant.core import HomeAssistant, callback
from homeassistant.helpers import config_validation as cv
from homeassistant.helpers import service

from .const import DOMAIN


@callback
def async_setup_services(hass: HomeAssistant) -> None:
    """Register the four official camera entity services once."""
    definitions = (
        ("record", None, "record"),
        ("trigger_camera", None, "trigger_camera"),
        ("save_video", {vol.Required(CONF_FILENAME): cv.string}, "save_video"),
        (
            "save_recent_clips",
            {vol.Required(CONF_FILE_PATH): cv.string},
            "save_recent_clips",
        ),
    )
    for name, schema, method in definitions:
        service.async_register_platform_entity_service(
            hass,
            DOMAIN,
            name,
            entity_domain=CAMERA_DOMAIN,
            schema=schema,
            func=method,
        )
