# ADR 0013: Device health entities and Blink re-login

- Status: accepted
- Date: 2026-10-04
- Amends ADR 0011 (USB status is also polled for HA entities)

## Context

Users uninstall the official Blink app and rely on Vistoda in Home Assistant.
Three app duties had no HA equivalent: telling the user that a camera or Sync
Module went offline, that the Sync Module USB drive needs attention, and that
Blink revoked the session. The engine already parsed camera and Sync Module
`status` strings and the native `local_storage/status` route, but only the
storage panel read USB status, and only while open. Both a Blink authentication
failure and a wrong local bridge token surfaced to HA as HTTP 401, so a bridge
misconfiguration started a pointless Blink sign-in, and the engine treated any
failed token refresh (including network outages) as a revoked session.

## Decision

### Connectivity

The engine adds `online: bool | null` to every camera and network in
`/v1/state`, plus `has_sync_module` per network (the module ID stays private).

| Blink `status` | `online` |
| --- | --- |
| `online`, `done` (case-insensitive) | `true` |
| `offline` | `false` |
| missing or any other value | `null` (raw `status` stays an attribute) |

`done` is what battery cameras report after a completed check-in; the public
blinkpy client uses the same table. For networks, the Sync Module's own status
wins over the network object's status. HA adds a `CONNECTIVITY` binary sensor
named "Connessione" per camera (unique ID `{camera serial}-online`) and per
Sync Module (unique ID `vistoda-{module serial}-online`). Mini-only systems get
no extra network sensor: their device is the camera itself.

### Sync Module USB health

A dedicated HA coordinator reads the status-only `GET /v1/local-storage/status`
every **5 minutes** (one Blink request per Sync Module; the USB manifest is
never rebuilt) and immediately after a successful eject, mount or format from
the panel. It exists only when a Sync Module is present. A removed or unplugged
drive therefore shows up within one interval.

The ENUM sensor "Archivio USB" (`vistoda-{serial}-usb_storage`) and the
`PROBLEM` binary sensor "Problema archivio USB"
(`vistoda-{serial}-usb_storage_problem`) map the engine's fields as follows:

| Engine `usb_state` and fields | Sensor state | Problem |
| --- | --- | --- |
| `active` | `ok` | off |
| `active` with `storage_warning >= 3` | `almost_full` | off |
| `active` with `usb_storage_full` | `full` | on |
| `memory_full` | `full` | on |
| `format_required` | `format_required` | on |
| `unavailable` (no drive detected) | `removed` | on |
| `unmounted` (safely ejected) | `unmounted` | on |
| `incompatible` | `incompatible` | on |
| empty or unrecognized value | unknown | unknown |
| any state with `last_backup_result` containing `fail`/`error` | unchanged | on |

Blink's `unavailable` becomes `removed` because `unavailable` is reserved by
Home Assistant. Unrecognized values stay unknown rather than mirroring the
official app's "treat as incompatible" rule, so a new healthy Blink state never
raises a false alarm. Attributes: raw `usb_state`, `percent_used`,
`storage_warning`, `storage_full`, backup flags, `last_backup_completed`,
`last_backup_result`, `backup_failed` and `network_id`. Both entities are
unavailable when the status request fails or Blink returns no entry for the
module.

### Re-login

- HTTP 401 now means only "bridge token rejected". The adapter raises a normal
  update failure and opens the non-fixable repair issue
  `bridge_token_rejected`; it never starts a Blink sign-in for it.
- A Blink-side authentication failure that a refresh cannot fix returns HTTP
  403 with `{"error":"reauth_required"}`. The adapter maps exactly that pair to
  `ConfigEntryAuthFailed`, which starts the existing reauth flow. Any other 403
  (for example Blink rate limiting relayed as a cloud error) is a normal
  failure.
- The engine treats a refresh as revoked only when Blink's token endpoint
  answers 401, or 400 with OAuth `invalid_grant`. Transport errors, 5xx and
  other answers stay retryable HTTP 502 errors.

## Consequences

- The acceptance inventory grows by one binary sensor per camera and, per Sync
  Module, two binary sensors and one sensor. Existing unique IDs are unchanged.
- A 0.22 adapter against an older engine reports a Blink failure as a bridge
  token issue; adapter and engine ship together and must stay version-locked.
- Unconfirmed against live fixtures: camera `status` values other than
  `online` (taken from blinkpy) and the `last_backup_result` vocabulary. Both
  degrade to "unknown" or "no problem" rather than to false alarms, except a
  backup result containing `fail`/`error`.
