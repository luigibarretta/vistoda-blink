# ADR 0009: Walnut audio is a separate interoperability path

- Status: investigation; not production-enabled
- Date: 2026-09-12
- Scope: passive negotiation decoding and offline native-call analysis

## Correction to the earlier interpretation

ADR 0006's opening claim that the app no longer treats two-way audio as part
of the legacy path was too broad. The inspected Android 59.1 native Walnut
library contains an IMMI audio uplink. ADR 0008 correctly describes Cayuga
selection in that build, but its disabled WebRTC policy is not evidence that
Walnut cannot send audio. Do not bypass the Cayuga gate on this basis.

The TLS correction preserves already-working live playback and authenticates
the IMMI server. It does not implement the missing audio transport or prove
any camera's simultaneous listen/talk capability.

## Observed contract

Analysis used the authenticated ARM64 library identified in PROVENANCE.md.
Only interface observations and original implementation are published, not
the binary, disassembly or copied native implementation.

- IMMI uses a nine-byte header: type, big-endian 32-bit value, big-endian
  32-bit payload length.
- Incoming header-only type `0x0c` carries the audio configuration in the
  value field. The native client validates it and invokes its two-way audio
  configuration callback. It is not an audio frame or permission to capture.
- `sendAudioConfig` emits type `0x0c`, a format value and an empty payload.
- `submitAudioFrame` reaches `sendAudio`, which emits type `0x05` with an
  audio packet counter and the encoded payload through `sendMediaInfo`.
- `requestMicrophone` and `relinquishMicrophone` invoke the microphone
  enable/disable path. That path enables/disables the audio input pipeline.
- The microphone enable path mutes audio output unless both the stream's
  echo-cancellation indication and a local audio-capability flag are true.
  The stream indication is derived from the native audio-format word;
  the local flag comes from `AudioCapabilityHelper`, not the TLS certificate.

This is static evidence for a conditional simultaneous-audio path, not proof
that a browser or a particular deployed camera satisfies its prerequisites.
An advertised account capability alone is insufficient.

## Implemented research boundary

`ImmiDecoder::push_events` preserves header-only audio configuration events,
including fragmented headers. Unknown format values remain raw observations;
they are not classified as supported. Unexpected payload-bearing `0x0c`
messages are not treated as a valid offer. Existing `push` consumers still
receive only video, preserving the live playback contract.

No uplink writer, microphone acquisition, provider request or production
capability change is introduced. Offline fixtures verify fragmentation,
unknown values, malformed offers, truncation and unchanged video filtering.
They do not constitute a native-client or physical-camera interoperability test.

## Remaining acceptance gates

1. Observe the actual powered Mini's audio offer without sending audio.
2. Resolve its exact codec, framing, pacing and enable/disable negotiation;
   do not assume browser Opus packets match the native format.
3. Implement a bounded owner-bound uplink sharing the existing camera lease,
   with explicit microphone gesture, mute, disconnect cleanup and no buffered
   audio replay after release/reconnect. Preserve verified TLS throughout.
4. Validate local echo cancellation and latency; otherwise expose only an
   accurately labelled push-to-talk mode, not full duplex.
5. Run a user-coordinated short audible test and verify simultaneous inbound
   and outbound audio, microphone conflicts and complete teardown.

No synthetic audio should be sent to household speakers without advance notice.
Do not activate battery cameras or change movement/alarm settings for research.
Technical evidence does not settle acquisition provenance or legal clearance.
