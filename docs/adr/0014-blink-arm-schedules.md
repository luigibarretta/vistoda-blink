# ADR 0014: Blink arm/disarm schedules (programs)

- Status: accepted
- Date: 2026-10-04

## Context

Blink stores arm/disarm schedules ("programs") on its servers. They keep
running after the official app is uninstalled, so a schedule created years ago
can silently disarm a network that Home Assistant just armed (or the reverse),
and without the app the user cannot even see it.

The Android 59.1 app (`device.network.program.ProgramApi`) uses, relative to
the regional REST host:

| Call | Route | Body / answer |
| --- | --- | --- |
| list | `GET api/v1/accounts/{a}/networks/{n}/programs` | JSON array of `scheduling.Program` |
| enable | `POST …/programs/{p}/enable` | no body; answer ignored (`Unit`) |
| disable | `POST …/programs/{p}/disable` | no body; answer ignored (`Unit`) |
| create, update, delete | `…/create`, `…/{p}/update`, `…/{p}/delete` | not used by Vistoda |

`Program` (Gson field names) has `id`, `network_id`, `name`, `format`,
`status` and `schedule`; `isEnabled()` is true for every status except
`disabled` (and a missing one). Each `ScheduleAction` has `action`
(`arm`/`disarm`), `time` (`yyyy-MM-dd HH:mm:ss Z`, written in UTC), `dow`
(three-letter UTC weekdays) and `devices` (camera IDs). The app converts to
local time by moving the anchor date to each weekday inside its Monday-based
week and then to the network time zone.

## Decision

- The engine reads every network's programs during the existing state refresh,
  at most every 10 minutes (one GET per network), so a five-minute HA refresh
  adds no Blink request in between. A failing network keeps its previous
  programs; HTTP 404 means "no programs" and is logged at debug level only.
- Programs are part of `/v1/state` (`programs`) and of `GET /v1/programs`.
  Names are bounded to 64 printable characters, schedules to 32 actions,
  lists to 32 programs; camera IDs are reduced to a `device_count`.
- `POST /v1/networks/{n}/programs/{p}/enabled {"enabled": bool}` validates
  numeric IDs and the network, serializes with the settings writes, reads the
  program first (no-op when already in the requested state), calls the native
  enable/disable route and reads the list back up to three times one second
  apart. A mismatch returns HTTP 502 (`SettingsVerification`); an unknown
  program is HTTP 404. Logs never include program names or IDs.
- The adapter adds one `switch` per program on the network (alarm panel)
  device: name `Programma: <name>`, entity category `config`, unique ID
  `vistoda-{sync serial or network-ID}-program-{id}`, so the entity ID is
  `switch.<network>_programma_<name>`. Attributes: `program_id`, `network_id`,
  `status`, `schedule` (`day`/`time`/`action` in HA's time zone, at most 64)
  and an Italian `summary` such as `lun-ven 22:00 arma; lun-ven 07:30 disarma`
  (at most 240 characters). After a toggle the adapter merges the engine's
  verified copy instead of triggering another Blink refresh. New programs gain
  a switch without a reload (at most 64).
- No create, edit or delete: those need the full `UpdateProgramRequest`
  round-trip and are out of scope.

## Consequences

- Users without the app can see and stop schedules that undo HA arming.
- A program changed in the Blink app shows up within about 10 minutes.
- The schedule summary uses HA's time zone, not Blink's network time zone; for
  a single-home installation these are the same.
- The response schema is modeled on the decompiled classes; it is not yet
  confirmed against the live account (fixtures are synthetic).
