# ADR 0015: Faster motion notifications and local motion stills

- Status: accepted (extends ADR 0012)
- Date: 2026-10-04

## Context

With ADR 0012 a motion event reached Home Assistant after Blink published it
plus up to 30 s of engine polling and up to 15 s of HA polling. Rich
notifications also need a still, but Blink leaves the v4 `thumbnail` empty for
some events; on 2026-10-04 the first no-subscription event had none.

Blink publishes no rate limit. Field experience (repeated canaries) shows HTTP
403, and the generic HTTP 429, when a client polls too often; the official app
reads the event list only while it is open, and push does the rest.

## Decision

Polling cadence (engine, `motion_poller.rs`):

| Condition | Wait before the next v4 list read |
| --- | --- |
| any network armed | 15 s ± 3 s random jitter, never below 12 s (`fast`) |
| armed, within 30 min of a Blink 429/403 | 30 s, the 0.20–0.22 behavior (`conservative`) |
| all disarmed | 300 s (`idle`) |
| request failure | 60 s, doubling to at most 900 s (`backoff`) |

One armed account therefore makes about 4 list reads per minute (was 2), each
at most three pages. `GET /v1/motion` reports `poller.mode`,
`poller.interval_seconds` (the effective wait, jitter included) and
`poller.rate_limited_until`. Only the motion poller reacts to the rate limit;
the state refresh keeps its own cadence.

Delivery to HA: `/v1/motion` returns a `sequence` that changes when a camera's
latest event, its Blink thumbnail or its local still changes. With
`?since=<sequence>&wait=<s>` the engine holds the request (at most 30 s) until
the sequence differs. The adapter keeps one 25 s long-poll open (40 s client
timeout, 15 s retry after errors) and pushes each change into the motion
coordinator at once; the 15 s interval refresh stays as a fallback and still
ends the 90 s motion window. Providers without `sequence` are only polled.
Long-polls never call Blink.

Local still (`motion_still.rs`): when a motion event starts an HA-local motion
recording (ADR 0012, opt-in, armed network, selected camera) or arrives while
that camera's recording is running, the engine joins the existing live session
(it waits up to 10 s for the recorder's publisher and never starts a session or
wakes a camera itself) and pipes its MPEG-TS into the bundled FFmpeg: keyframes
only, first frame scaled into 1280×720 at full range, one MJPEG on stdout.
Bounds: 8 MiB input, 1 MiB JPEG (checked SOI/EOI), 30 s, two concurrent
decoders (further events are skipped), latest 16 stills in memory. The still is
keyed by the Blink event ID: `/v1/motion` then reports
`thumbnail_available: true` and `thumbnail_source: "local"`, and
`/v1/motion/events/{id}/thumbnail.jpg` serves it. Blink's own thumbnail is
preferred (`thumbnail_source: "blink"`); the local still is also the fallback
when downloading Blink's fails. FFmpeg gains the H.264 decoder, MJPEG
encoder/muxer and `scale`/`format` (libswscale); it remains an LGPL,
pipe-only build (docs/AUDIO_CODEC.md).

## Consequences

- The polling share of the latency from Blink publication to the HA sensor
  drops from at most 45 s (30 s + 15 s) to at most 18 s (about 8 s on
  average); the long-poll removes the HA polling share.
- A still is ready a few seconds after the motion recording's live session
  delivers its first keyframe, inside the 60 s the HA automation waits.
- The still shows the scene after detection plus live start-up, not the
  trigger moment. Without motion recording (disabled, unselected camera or a
  disarmed network) behavior is unchanged: only Blink's thumbnail is used.
- Stills live in memory and disappear on restart, as do notifications'
  short-lived references to them.
- A still subscriber can keep a running live session alive for at most its
  30 s budget after the recording ends; normally it finishes within seconds.
- Twice the armed polling raises the risk of Blink throttling. The rate-limit
  hold returns to the proven 30 s cadence for 30 minutes after each 429/403;
  the exact Blink thresholds remain unknown.
