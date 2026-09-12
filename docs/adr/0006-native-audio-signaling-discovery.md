# ADR 0006: Native audio signaling discovery

- Status: accepted
- Date: 2026-09-09

## Context

Scope correction (2026-09-12): the opening interpretation below is superseded
by [ADR 0009](0009-walnut-audio-investigation.md). Native Walnut also contains
an IMMI audio uplink; the WebRTC policy does not rule out that separate path.

The Blink Android 59.1 client no longer treats two-way audio as part of the
legacy live MPEG-TS path. Its v4 homescreen supplies a private Ring device ID
and `two_way_audio`, `audio_aec` and `audio_privacy_enabled` capability flags.
The app then uses the shared Ring WebRTC signaling service with protocol 4.1,
`blink_oauth`, the OAuth access token and a client ID derived from the Blink
hardware identity and the account's `ringUserId` (not its Blink `userId`).

Knowing the WebSocket handshake is not evidence that Vistoda can safely talk.
A production control additionally needs the exact SDP offer/answer fields, ICE
servers, negotiated codec, microphone uplink, speaker downlink, device ownership,
timeouts and reliable teardown/recovery behavior.

## Decision

The engine discovers and retains the private Ring device ID only in memory; it
is excluded from serialized camera state. The public state may expose only the
non-secret audio capability flags returned by the v4 homescreen.
The binary installs rustls's Ring process-level crypto provider before either
the REST or WebSocket TLS client is constructed.

An authenticated diagnostic endpoint constructs the native 4.1 signaling
handshake and connects to the fixed production host. It reports only whether
authentication succeeded, whether a Ring identity exists and whether the camera
advertises two-way audio. A failed upgrade returns only a bounded failure phase
and HTTP status, never response bodies or credentials. It sends no live, SDP,
ICE, microphone or speaker message, starts no media session and closes
immediately after the upgrade.

Vistoda implements the RMS v4 session as a signaling-only broker. The browser
creates a balanced Unified Plan peer connection with audio `sendrecv` and video
`recvonly`, then sends its offer through an owner-bound Home Assistant
subscription. Rust translates only typed `live_view`, SDP, ICE,
`activate_session`, `stream_options`, `camera_options`, `mic_enable`, ping and
structured close messages. WebRTC media remains end-to-end between browser and
Blink; no Rust media stack or transcoder is added.

A single per-camera lease is shared by WebRTC, legacy IMMI live and recordings.
Sessions are bounded by message/candidate limits, a six-minute deadline, ping
timeouts and connection cleanup. Additional Blink content-encryption SDP fails
closed. Speaker and microphone start disabled, are controlled independently and
the browser microphone uses a Vistoda-wide exclusive lock shared with Ring.

## Consequences

- Authentication and capability discovery can be verified independently of a
  camera wake-up or microphone capture.
- OAuth tokens and private Ring device IDs never enter Home Assistant panel
  state or probe output.
- Browser SDP, ICE and audio state no longer expose OAuth or private Ring device
  IDs to Home Assistant.
- A successful WebSocket/SDP negotiation is not by itself a full-duplex claim;
  every camera family still needs a powered-camera media and mute canary.
- Provider busy, unsupported codec, E2EE and microphone override conditions fail
  closed and remain visible to the user.
