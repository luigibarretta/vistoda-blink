# ADR 0012: Motion events and USB-free motion recording

- Status: accepted
- Date: 2026-10-04

## Context

On 2026-10-03 the official app listed 11 motion events in its cloud clip list
while Home Assistant saw none. Vistoda read the legacy
`api/v1/accounts/{id}/media/changed` feed, dropped every event without a media
URL (Blink reports `no_subscription` events without video) and marked motion
only for clips younger than two minutes at a five-minute coordinator refresh.
The official Android 59.1 app reads `POST v4/accounts/{id}/media` instead.

Accounts without a Blink subscription only get video when a Sync Module USB
drive works. Users asked for motion clips without a USB drive.

## Decision

- The engine owns a lightweight motion poller, independent of the heavy state
  refresh. While any network is armed it reads the native v4 media list every
  30 seconds from a persisted cursor; disarmed networks are read every five
  minutes. HTTP 403/429 back off exponentially up to 15 minutes.
- Every new event, with or without video, becomes a motion event keyed by its
  Blink media ID. A camera reports motion for 90 seconds after its last event.
  `GET /v1/motion` serves this cached state without calling Blink, so the HA
  adapter can poll it every 15 seconds.
- Optional motion recording (disabled by default, administrator setting
  persisted by the engine) starts one bounded HA-local recording (15, 30 or 60
  seconds) on the selected cameras for a new event while its network is armed.
  It reuses the recording manager, so one recorder per camera, the per-file
  limit and the spool quota still apply. Recordings carry `trigger: motion`.
- Motion recordings are a rolling buffer: when the spool lacks headroom, the
  oldest motion-triggered recordings are removed first. Manual recordings are
  never removed automatically.
- The HA-owned hourly NFS backup also copies HA-local recordings, so motion
  clips reach the NAS without a USB drive.

- Events can carry Blink's `thumbnail` (the still behind the official app's
  rich notification), sometimes only in a later poll. The engine keeps the
  thumbnail path of the latest 256 events, accepting only relative Blink paths
  or HTTPS URLs on `*.immedia-semi.com`, and serves it at
  `/v1/motion/events/{id}/thumbnail.jpg`. The adapter re-serves it to
  authenticated HA users, so Companion notifications can attach it, and the
  motion sensor exposes the relative URL as its `thumbnail` attribute (0.21.0).

## Consequences

- Motion latency is bounded by Blink publishing the event plus up to 30 s of
  polling and 15 s of HA polling. A motion recording therefore captures the
  scene after the event, not the trigger itself; Blink's own clip (USB or
  cloud) remains the only record of the trigger moment.
- Each motion recording wakes the camera for a live session. Battery cameras
  drain faster; the camera selection lets users exclude them.
- Blink rate limits apply. The poller stops on authorization failures and
  never retries faster than the backoff.
