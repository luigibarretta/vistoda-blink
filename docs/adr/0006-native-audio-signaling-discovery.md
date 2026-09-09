# ADR 0006: Native audio signaling discovery

- Status: accepted
- Date: 2026-09-09

## Context

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

The Vistoda microphone control stays hidden. It may be enabled only after a
bounded live canary proves simultaneous downlink and uplink, user-gesture-based
microphone permission, mute/unmute, teardown and recovery on every supported
camera family.

## Consequences

- Authentication and capability discovery can be verified independently of a
  camera wake-up or microphone capture.
- OAuth tokens and private Ring device IDs never enter Home Assistant panel
  state or probe output.
- A successful probe narrows the remaining work but is not a claim of
  full-duplex support.
- The official Blink app remains required for two-way talk until the media
  contract passes the live canary.
