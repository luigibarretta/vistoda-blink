# ADR 0008: Exact Cayuga-to-Walnut live fallback

- Status: accepted
- Date: 2026-09-10

## Evidence

Blink Android 59.1 constructs a Walnut manager directly when
`WEBRTC_LIVE_VIEW` is disabled. When the flag is enabled it constructs a
`FallbackLiveViewSessionManager`, initially backed by
`CayugaWebRtcLiveViewSessionManager`, caches the live request and observes only
the first `UnrecoverableTerminationReason`.

The two reasons declared by the app are:

- `NoRingDeviceId`, emitted before a WebRTC session when the camera has no
  shared Ring identity;
- `BlinkLegacyDevice`, emitted only when the Ring streaming layer terminates
  with `ReleaseReason.BLINK_LEGACY_DEVICE`.

The bundled Ring signaling model maps provider close code `38` to that release
reason and describes it as requiring the IMMI/RTSP path. Close code `2` is
`SESSION_SETUP_FAILED`; it maps to `REMOTE_INIT_FAILED` and does not trigger the
official automatic fallback.

On an eligible reason the app changes manager once, rebinds callbacks and its
renderer, then starts Walnut with the cached request. It does not loop back to
Cayuga or fall back on arbitrary transport, SDP, ICE or media failures.

## Decision

Vistoda follows the same one-way policy:

1. A camera without a private Ring device identity rejects the local WebRTC
   upgrade with a dedicated, secret-free precondition status.
2. Provider close code `38` is translated to a typed
   `blink_legacy_device` fallback event. Provider text is never relayed.
3. Every other close remains a WebRTC error. In particular, code `2` is not
   silently reclassified.
4. The Home Assistant panel fully tears down its owner-bound WebRTC session
   before starting the existing IMMI stream through Home Assistant's standard
   camera player. The transition can happen only once per user start.
5. Walnut mode exposes no Vistoda microphone control. A non-eligible WebRTC
   failure may offer an explicit compatible-live action, but never starts a
   hidden second provider session automatically.

The per-camera engine lease continues to exclude simultaneous WebRTC, IMMI and
recording publishers. Missing identity and code `38` are covered by offline
fixtures; provider close code `2` is a negative fixture.

## Consequences

- Existing IMMI live remains available without weakening the WebRTC error
  boundary.
- A provider-side protocol error cannot cause an unbounded retry or wake cycle.
- Full-duplex audio remains available only after a verified WebRTC connection;
  Walnut preserves video compatibility but is not advertised as two-way audio.
- No OAuth token, private Ring identifier or provider close text enters panel
  state, tests or logs.
