# Vistoda Blink Engine

This supervised Rust app is installed automatically with Vistoda Blink. It has
no public port. It independently owns Blink enrollment, token refresh,
discovery, state, controls, snapshots, clips and live IMMI sessions. The small
Home Assistant integration talks only to its private API; the official Blink
integration is not required.

The app reports Sync Module status, firmware and provider storage-used values.
It can delete one revalidated USB clip, format provider-declared compatible
media, and safely eject or remount the USB drive; each of these mutations is
written to the app log with its network, Sync Module and result. Wi-Fi
migration and Sync Module removal have no engine route: the Vistoda panel may
show them as unavailable so users know to use the official Blink recovery flow.

Enrollment is normally completed from **Settings → Devices & services → Add
integration → Vistoda Blink**. Email and password exist only for the duration
of the OAuth exchange. The long-lived refresh token is sealed in
`/data/provider.sealed` with a key derived from the workload token, which is
stored beside it in `/data/workload-token`. An app backup therefore contains
both the sealed token and its key: protect app backups like the Blink account
itself. Blink rotates the refresh token on every refresh; if the new token
cannot be written, the app keeps it in memory, retries the write and reports
`credentials_persisted: false` in `/healthz` until disk and memory agree.

The `token` option is managed as a secret by the production deployment. It must
contain exactly 64 lowercase hexadecimal characters. Do not publish it or add a
host port for the engine. Production pins engine and adapter versions
independently so an adapter-only fix does not rebuild the Rust app.

Named camera-settings backups live in Home Assistant storage, not in this app's
credential file. Restore still uses this app's typed setting endpoint and
optimistic provider revisions; it never copies OAuth material into a settings
backup.

Blink arm/disarm programs keep running on Blink's servers after the official
app is removed. The app reads them every 10 minutes and Home Assistant shows
one "Programma: …" switch per program to enable or disable it (ADR 0014).
While a network is armed, motion events are read every 15 seconds (30 seconds
for half an hour after a Blink rate limit). After a failed read the poller
backs off to at most two minutes while armed, and to at most 15 minutes after
a rate limit or while disarmed. With motion recording enabled, a
JPEG still from the recording backs notifications when Blink sends no event
image (ADR 0015).
