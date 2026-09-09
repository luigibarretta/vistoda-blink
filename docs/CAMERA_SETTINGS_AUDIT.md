# Blink camera settings audit

Audit date: 2026-09-09

## Outcome

Vistoda Blink can replace the official app for routine viewing and control, but
cannot yet replace it for full camera administration. The safe route is an
incremental, model-aware settings API with read-back verification. Blindly
forwarding undocumented JSON would risk silently applying a setting to the
wrong field or device generation.

The reference is Home Assistant Core 2026.9.1 with `blinkpy` 0.25.9, plus the
official Blink screens supplied for this audit. No production setting was
changed while gathering evidence.

## Current coverage

Vistoda already owns OAuth2/2FA, discovery, arm/disarm, motion enable/disable,
motion state, record clip, fresh or cached snapshot, saved recent clips and
bounded live MPEG-TS. Its camera state includes name, serial, firmware, product
type, battery, temperature, Wi-Fi strength and online/power information.

The official Home Assistant integration exposes a similar administrative
subset: arm/disarm, motion control, snapshot/record/save commands, cached camera
images, temperature, Wi-Fi and battery state. It explicitly does not provide
live viewing. It does not expose the advanced settings below as HA entities or
services.

## Feasibility by official-app surface

| Official setting | Status for Vistoda | Required work and safety gate |
| --- | --- | --- |
| Motion detection | Available | Surface the existing switch in the Vistoda camera detail view. |
| Battery, firmware, temperature | Available read-only | Render existing provider state; add temperature-alert thresholds separately. |
| Network and Sync Module strength | Mostly available | Expose current Wi-Fi and parsed Sync Module diagnostics with units and stale-state handling. |
| Record clip and refresh thumbnail | Available | Reuse the existing bounded commands and show completion/read-back. |
| Camera name | Read available; rename not implemented | Add a model-aware rename command, collision checks and read-back before exposing edit. |
| Night vision | Limited upstream precedent | `blinkpy` implements config read/update only for its `owl` and `catalina` product paths. Prove each enrolled model before enabling. |
| Clip length and video quality | Research required | Discover allowed values per product; validate range/enum and re-read after mutation. |
| End clip early | Research required | Map the exact per-model config key and retain the previous value for rollback. |
| Motion sensitivity and retrigger time | Research required | Map ranges and units per generation; debounce sliders and verify the committed value. |
| Early notification | Research required | Confirm whether this is camera, network or account scoped and whether subscription state changes behavior. |
| IR intensity | Research required | Couple to night-vision capability and reject unsupported product types. |
| Video recording and audio streaming | Privacy-sensitive research | Require explicit confirmation, model capability discovery and immediate read-back. |
| Photo Capture and Auto-Update Thumbnail | Research required | Confirm plan, armed-state and product restrictions; never imply success from HTTP status alone. |
| Status LED | Research required | Identify supported modes per model and expose only returned enum values. |
| Activity zones | High complexity | Define the device grid/coordinate schema, snapshot revision and atomic update/reset behavior. |
| Privacy zones | High privacy risk | Same editor requirements as activity zones, plus fail-closed masking verification. |
| Speaker volume | Not supported by current provider | Requires an authenticated audio-settings endpoint and verified scale per model. |
| Two-way Blink audio | Not supported | Requires a new talk-media path, microphone consent and session lifecycle; live video alone is insufficient. |
| Temperature alerts | Not supported | Determine whether thresholds are device or notification-account state and validate units. |
| Delete device | Deliberately deferred | Destructive account-topology action needs reauthentication, typed confirmation and recovery guidance. |

## Delivery plan

1. Add a Vistoda camera-detail view using the already trusted state and actions:
   motion, battery, firmware, temperature, Wi-Fi, snapshot, record and live.
2. Introduce a read-only, redacted capability/config endpoint in the Rust
   provider. Persist no raw vendor response and log no account or device secret.
3. Capture fixtures for every enrolled product type and define typed enums,
   ranges and feature flags. Unknown fields remain hidden.
4. Implement one setting family at a time with an optimistic-concurrency token,
   write/read-back comparison, bounded retry and restoration of the prior value
   when verification fails.
5. Build zone editing only after its coordinate and revision contracts are
   proven. Test privacy masking visually on every supported generation.
6. Keep rename, notification administration, audio/talk and device removal in
   the official app until their provider contracts and recovery paths pass live
   canaries.

## Uninstall criterion

The official Blink app can be considered optional only after the user's actual
camera models pass live read/write/read-back tests for every setting they use,
and after privacy zones, notification behavior, account/device lifecycle and
two-way audio are either supported or explicitly accepted as unavailable.
Today, Vistoda is a strong daily-use replacement, not a complete administrative
replacement.

## Primary references

- Home Assistant Blink documentation:
  <https://www.home-assistant.io/integrations/blink/>
- HA Core 2026.9.1 Blink manifest and pinned library:
  <https://github.com/home-assistant/core/blob/2026.9.1/homeassistant/components/blink/manifest.json>
- HA Core 2026.9.1 Blink camera implementation:
  <https://github.com/home-assistant/core/blob/2026.9.1/homeassistant/components/blink/camera.py>
- `blinkpy` 0.25.9 camera config behavior:
  <https://github.com/fronzbot/blinkpy/blob/v0.25.9/blinkpy/camera.py>
- `blinkpy` 0.25.9 request paths and update constraints:
  <https://github.com/fronzbot/blinkpy/blob/v0.25.9/blinkpy/api.py>
