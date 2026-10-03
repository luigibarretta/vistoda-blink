# ADR 0011: Native Sync Module USB state parity

- Status: accepted
- Date: 2026-10-03
- Amends ADR 0007 (eject and mount are no longer absent)

## Context

On 2026-10-03 the production Sync Module reported `usb_state:
format_required`. The official app opened its "Format USB Drive" screen, while
Vistoda showed a 0 % usage gauge and an empty index. Blink Android 59.1
(`device/sync`) shows that the official feature is a state machine over six
`LocalStorageState` values (`active`, `format_required`, `unmounted`,
`unavailable`, `memory_full`, `incompatible`; any unknown value is treated as
`incompatible`), refreshed every 30 s while a storage screen is open and after
each command. Commands are polled every 2 s and succeed only when
`complete && status == 0` (`SupervisorKommand.isSuccessful`). Vistoda accepted
any `complete` command, so a rejected format could be reported as a success.

## Decision

- Storage commands fail with HTTP 409 when Blink completes them with a non-zero
  status and with HTTP 504 when Blink has not confirmed them within the bounded
  90 s wait. The panel re-reads status instead of assuming either outcome.
- `GET /v1/local-storage/status` returns status only, so the 30 s refresh never
  asks the Sync Module to rebuild its manifest.
- `storage_warning` is exposed; the panel follows the native `>= 3` "almost
  full" banner, the `>= 90 %` red usage and the backup-in-progress banner.
- Safely eject (`active`/`memory_full`) and reconnect (`unmounted`) use the
  native `eject`/`mount` commands. They are a reversible pair, need an HA
  administrator, have no typed confirmation (as in the official app) and are
  revalidated against fresh provider status before sending.
- Formatting keeps the ADR 0007 typed confirmation and the
  `usb_format_compatible` guard, stricter than the official app, because it is
  irreversible.
- Delete-all, Wi-Fi change and Sync Module removal remain absent.

## Consequences

- A failed or unconfirmed provider command can no longer be shown as success.
- Eject and mount are exact APK contracts but were not exercised live at
  release time because the production drive needed formatting; the first live
  use must be an explicit eject→mount canary on a healthy drive.
