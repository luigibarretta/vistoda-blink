# Blink camera settings audit

Audit date: 2026-09-09

## Outcome

Vistoda Blink now replaces the official app for routine viewing and the verified
camera-administration surface used by this installation. It still cannot claim
complete replacement because Blink two-way media negotiation, destructive
device removal and Owl/Mini v2 zones do not have a proven recoverable contract.

The references are Home Assistant Core 2026.9.1 with `blinkpy` 0.25.9, the
supplied official screens, and Blink Android 59.1 build 29797423. The audited
base APK SHA-256 is
`77e6fefb8dbbd68f1e964e0822de14d4a5408e9fe5f648eb7d5cce19238e12d7`.
Production mutations are limited to reversible canaries which immediately
restore the original provider value.

## Current coverage

Vistoda owns OAuth2/2FA, discovery, arm/disarm, motion control and state, cloud
clip access, fresh or cached snapshots, saved recent clips and bounded live
MPEG-TS. Version 0.8.0 also records 15, 30 or 60 seconds directly from that
shared live stream into a separate, quota-bounded Vistoda archive. It does not
manufacture a Blink motion event.
Camera state includes name, serial, firmware, product type, battery, temperature,
Wi-Fi strength and online/power information.

The official Home Assistant integration exposes a smaller administrative subset
and no live viewing. Its advanced settings remain official-app-only; Vistoda's
advanced surface is implemented by the standalone Rust provider.

## Feasibility by official-app surface

| Official setting | Vistoda status | Safety contract |
| --- | --- | --- |
| Motion detection | Available | Current value, HA switch and typed setting. |
| Battery, firmware, temperature | Available read-only | Current provider state. |
| Network and Sync Module strength | Available where returned | Units and unavailable state retained. |
| Record clip and refresh thumbnail | Available | Bounded commands with completion/read-back. |
| Camera name | Available | Stable ID alias survives verified provider rename. |
| Night vision and IR intensity | Available | Returned values; exact IR levels 1/4/7. |
| Clip length and video quality | Available | Per-model range and provider enum validation. |
| End clip early | Available where returned | Read-back and prior-value restoration. |
| Motion sensitivity and retrigger time | Available | Exact Owl/default keys, ranges and units. |
| Early notification | Available | Actual per-camera state; no assumed default. |
| Video recording and audio streaming | Available | Admin-only, confirmation and read-back. |
| Photo Capture and Auto-Update Thumbnail | Capability-gated | Shown only when returned by the camera. |
| Status LED | Available | Only model-valid returned modes. |
| Activity zones | Available on verified v1 cameras | Native grid, revision check and rollback. |
| Privacy zones | Available on compatible v1 cameras | Native spans, maximum two, fail-closed validation. |
| Speaker volume | Available on Mini/Owl | Android 59.1 proves integers 1–8 and `volume_control`. |
| Temperature alerts | Enable/disable available | Thresholds stay read-only until exact rules are proven. |
| Two-way Blink audio | Signaling discovered, media gated | Android 59.1 proves shared Ring WebRTC 4.1 and `blink_oauth`; Vistoda can authenticate and close without media, but SDP/ICE/uplink recovery remain unproven. |
| Sync Module USB clips | Available read-only in 0.9.0 | Status, manifest polling, bounded inventory and authenticated download; no delete/eject/format/mount route exists. |
| Delete device | Deliberately deferred | Requires reauthentication, typed confirmation and recovery. |

## Delivery result

1. Camera detail, trusted actions and current-state rendering: complete.
2. Redacted capability discovery and typed settings: complete.
3. Model-aware enums, ranges, feature flags and unknown-field hiding: complete.
4. Revision tokens, bounded verification and automatic rollback: complete.
5. Native v1 activity/privacy editor: complete in 0.7.0; Owl v2 remains gated.
6. Standalone local archive and checksum-verified NFS backup: complete in 0.8.0
   through the Vistoda Home Assistant control plane.
7. Sync Module USB inventory/download: complete in 0.9.0 without destructive
   provider controls.
8. Blink talk and device removal: intentionally gated pending media negotiation,
   reauthentication and recovery evidence.

## Version history

### 0.9.0

Upgraded discovery to the current v4 homescreen and preserves the private Ring
device identity and two-way-audio flags without serializing that identifier to
ordinary state. Added the exact Android 59.1 local-storage status, manifest,
command-poll and clip-request flow behind a bounded read-only API and a signed
Home Assistant download path. Destructive USB endpoints are absent by contract.

Added an authenticated signaling-only probe using the official app's shared
Ring WebRTC endpoint, protocol 4.1, `blink_oauth` and derived client identity.
The probe starts no live/media session and closes immediately. Full-duplex UI
remains hidden until a real SDP/ICE, downlink/uplink and teardown canary passes.

### 0.8.0

Added fixed-duration MPEG-TS recording from the already shared live hub, an
immutable manifest with byte count and SHA-256, crash recovery, per-camera
concurrency exclusion, a 96 MiB file ceiling and a 512 MiB spool quota. The HA
adapter exposes authenticated list, download and delete operations. Its legacy
`camera.record` hook now requests a 30-second local capture instead of invoking
the Blink cloud record command that could raise a false motion notification.

### 0.5.1

Added the camera-detail view and a Rust-owned redacted settings endpoint for the
first typed motion, recording, clip, video-quality, notification and night-vision
fields. Writes became administrator-only and gained optimistic concurrency,
validation, read-back and rollback attempts.

### 0.5.2–0.5.4

Added value-redacted capability and zone schemas, explicit current-state UI,
the three described Blink video-quality choices, and the bounded Android REST
identity/locale/time-zone header contract required by newer endpoints.

### 0.6.0

Added camera rename, LED mode, IR intensity, compatible rotation, Photo Capture,
observed Mini speaker volume and the separate temperature-alert action. Fixed
Owl clip/retrigger keys and integral JSON floats. Persisted aliases by device ID
so rename cannot break entities or media routes. Zone discovery now follows the
returned `zone_version` and fails closed to redacted config schema discovery.

### 0.7.0

The Android client proves speaker volume as integers 1–8 and the Owl writer key
as `volume_control`; the control now uses the same verification and rollback as
other settings.

Verified default/Catalina v1 cameras gain a native activity/privacy editor. The
grid is five by five basic cells, each with three rows by four columns: 20×15
micro-cells. Privacy spans are integer `x`, `y`, `w`, `h`, maximum two. Writes
preserve unrelated bits, clear activity beneath privacy spans, reject an entirely
disabled grid, compare the complete provider-response revision, reread the result
and restore the prior body if verification fails. Owl/Mini v2 stays hidden because
the enrolled Mini rejects that route; Vistoda does not guess a translation.

## Uninstall criterion

The official Blink app is optional only when every setting the user needs passes
live read/write/read-back tests on the enrolled model and the explicitly blocked
surfaces are acceptable. Today Vistoda is a daily-use and verified-settings
replacement. Keep the official app for Blink talk, destructive USB management,
device removal, unsupported v2 zones and future fields not returned by an
enrolled camera.

## Primary references

- Home Assistant Blink documentation:
  <https://www.home-assistant.io/integrations/blink/>
- HA Core 2026.9.1 Blink manifest:
  <https://github.com/home-assistant/core/blob/2026.9.1/homeassistant/components/blink/manifest.json>
- HA Core 2026.9.1 Blink camera implementation:
  <https://github.com/home-assistant/core/blob/2026.9.1/homeassistant/components/blink/camera.py>
- `blinkpy` 0.25.9 camera behavior:
  <https://github.com/fronzbot/blinkpy/blob/v0.25.9/blinkpy/camera.py>
- Legacy Blink API notes for the 25-cell mask:
  <https://github.com/adrian-dobre/BlinkWebService/blob/master/BlinkForHomeApiDocumentation.md>
