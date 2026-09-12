# Per-camera temperature alerts

Requires Vistoda Blink 0.14.0 and the Vistoda Home Assistant panel 0.27.0.
Open a supported camera's **General settings**. Its alert switch is read from
Blink, not inferred from recent notifications. The cold/hot thresholds are also
read from Blink. If unset upstream, they remain empty until the user enters both.
The original Mini does not support this surface.

The panel uses Home Assistant's temperature unit. Blink stores integer °F, so
values entered in °C may round by a fraction of a degree. Supported outdoor
controls accept −4…113 °F (−20…45 °C); Catalina Indoor uses 32…95 °F (0…35 °C).
Existing upstream values outside those input ranges are displayed unchanged.
New thresholds require at least a 10 °F gap. Direct sunlight can affect readings.

Edits are staged until **Save changes** and confirmation. For configured cameras,
expanding a range happens before shrinking it; read-back verifies each write.
Initialization submits both explicitly chosen thresholds together and requires
fresh `signals.temp`, echoed as `current_temp` to preserve calibration. Missing
telemetry disables editing. Calibration adjustment itself is not exposed here.
First setup is kept separate from unrelated settings because Blink has no known
operation to restore an absent threshold. If verification fails, the UI reports
uncertainty and reloads; it must not claim a successful rollback of first setup.

The alert enable/disable operation configures **Blink's native push service**.
The Blink app must have notification permission. Home Assistant Companion push
delivery is not implemented by this setting. Tests do not heat/cool hardware or
change the user's alert thresholds to manufacture a notification.

The adapter accepts a bounded object only for paired initialization:
`key: temperature_thresholds`, `value: {temperature_min: 32, temperature_max: 95}`,
plus the current revision. Ordinary saved thresholds remain individual typed
settings. Neither the browser nor this document contains vendor credentials.
See [protocol provenance](PROVENANCE.md) for observed upstream contracts and
[the settings audit](CAMERA_SETTINGS_AUDIT.md) for other model-dependent controls.
