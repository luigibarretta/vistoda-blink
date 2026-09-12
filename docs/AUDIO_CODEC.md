# Walnut audio codec build

Version 0.15.3 distinguishes single-client and multi-client live sessions using
the actual upstream descriptor. Multi-client audio requires an explicit available
session status before microphone activation; unknown policy is not authorization.
StartAudio and StopAudio are serialized with AAC on the same verified transport.
Disabling capture or receiving an audio-wait event revokes the local lease and
stops transmission; later availability never reactivates capture automatically.
These session messages are not proof of exclusive remote microphone ownership.
Speaker audibility and simultaneous conversation require separate hardware tests.
Passive video retains its five-second keepalive write tolerance; the 200 ms
control budget applies only while a microphone lease or its pending Stop exists.

Version 0.15.1 normalizes the outbound ADTS buffer-fullness field to zero to
match the native Walnut header. AAC payload, frame length, format and counters
are unchanged. Offline comparison of 17 synthetic encoded frames produced
identical decoded PCM before/after normalization. This is a compatibility
alignment, not proof of a diagnosed speaker fault or audible playback.

The audio implementation uses a separate, minimal FFmpeg command-line process.
The Rust engine does not link FFmpeg libraries. Its input and output use pipes;
network protocols and file input are disabled in the bundled codec build.
The selected build includes PCM16/AAC conversion and MPEG-TS/MP4 remux support.
The released microphone endpoint uses only PCM-to-AAC encoding: it does not
remux or replace Home Assistant's existing video player.

FFmpeg 9.0.1 is built from the unmodified release archive:

- Source: <https://ffmpeg.org/releases/ffmpeg-9.0.1.tar.xz>
- SHA-256: `cf38e0e28c7e5605942c4a77755349b0145804a397af37eb1fb4c77cb237f635`
- Release signature verified during dependency review against the fingerprint
  `FCF986EA15E6E293A5644F10B4322F04D67658D8` published on
  <https://ffmpeg.org/download.html>.
- License: LGPL-2.1-or-later; GPL, nonfree and automatic external-library
  discovery are explicitly disabled. This does not settle codec patent rights.

Every audio-enabled image contains the exact compressed source, LGPL text,
upstream license inventory, build package inventory and configuration at
`/usr/share/doc/vistoda/ffmpeg`. The source accompanies the executable in the
same image; it is not merely a link to a third-party server.

To rebuild or replace the codec, use the `audio-codec` stage in
`addon/vistoda_blink_engine/Dockerfile` and the included
`build-audio-codec.sh`. Keep the pipe/PCM/AAC contract when replacing the
executable at `/usr/local/bin/ffmpeg`. No binary signature or vendor key locks
this local codec to our build. The build recipe, generated configuration and
complete source are available to permit modifications and rebuilding.

The normal image runs the engine and its child processes as UID 10001.
No codec process should survive the session that owns it. No temporary media
file is needed for conversion. The decoder and encoder must have bounded
input queues; late audio must be discarded rather than replayed on unmute.

Capture is exactly 512 samples (32 ms) of PCM16LE/16 kHz/mono per block. The
encoder's initial 1024-sample priming frame is discarded. Each output frame
retains the oldest contributing PCM receipt timestamp, with a 200 ms deadline
through the final TLS write; late data is never relabelled as fresh. A slower
connection stops microphone transmission instead of replaying buffered speech.
Six offline runs passed this provenance check (worst observed age 176.32 ms).
These checks do not prove camera speaker output or full-duplex operation.

The authenticated `/v1/cameras/{alias}/walnut` WebSocket carries microphone
controls, exact PCM blocks and safe status only. It shares the existing camera
publisher with HA video. It starts an encoder only after explicit microphone
enable, a supported actual IMMI offer and an exclusive lease. `sent_frames`
counts completed IMMI writes, not playback acknowledgements from the camera.
Disconnect, disable, stale data and ownership changes revoke capture. A partial
TLS write must close the shared transport to prevent corrupt IMMI framing;
unlike an ordinary microphone-channel failure, that also affects live video.

The observed Mini offer does not advertise stream echo cancellation. The UI
must suspend local listening while talking and restore it on microphone disable.
This is push-to-talk, not verified simultaneous full duplex. Provider acoustic
acceptance must be tested separately on the target camera before claiming success.

FFmpeg's own licensing guidance: <https://ffmpeg.org/legal.html>.
