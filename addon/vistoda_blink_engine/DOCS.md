# Vistoda Blink Engine

This supervised Rust app is installed automatically with Vistoda Blink. It has
no public port. It independently owns Blink enrollment, token refresh,
discovery, state, controls, snapshots, clips and live IMMI sessions. The small
Home Assistant integration talks only to its private API; the official Blink
integration is not required.

Enrollment is normally completed from **Settings → Devices & services → Add
integration → Vistoda Blink**. Email and password exist only for the duration
of the OAuth exchange. The long-lived refresh token is sealed under `/data`
with a key derived from the separately managed workload token.

The `token` option is managed as a secret by the production deployment. It must
contain exactly 64 lowercase hexadecimal characters. Do not publish it or add a
host port for the engine. Keep app and adapter versions identical.
