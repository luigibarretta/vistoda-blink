# Vistoda Blink

Vistoda Blink is the private Blink provider and Home Assistant adapter for the
Vistoda product family. It provides supported camera state, controls, snapshots,
archives and bounded Walnut/IMMI live video.

The supervised Rust engine owns Blink OAuth2/2FA, token refresh, discovery,
polling, controls and media. The small Python custom integration is only the
native Home Assistant adapter. The official Blink integration is not required;
it can coexist temporarily as a parity oracle or one-time credential migration
source.

It does not claim parity with every Blink-app setting or camera model. Cayuga
WebRTC signaling is implemented, but microphone and full-duplex talk remain
disabled while the enrolled provider policy does not enable that transport.
Start a HAOS installation with the shared
[setup guide](https://github.com/luigibarretta/vistoda-addons/blob/main/GETTING_STARTED.md).

## Architecture

```text
Blink cloud -> supervised Vistoda Blink Rust engine
                         |             |
                         |             +-> private media API -> SceneTrove
                         +-> thin HA adapter -> native HA entities/services
    |
Vistoda for Home Assistant adopts the existing provider device
```

The stable Home Assistant domain remains `blink_live_bridge`. Keeping this
technical identifier preserves the existing config entry, camera entity IDs,
private API and Vistoda discovery contract during the product rename.

## Capabilities

- standalone OAuth2 PKCE enrollment, 2FA and sealed refresh-token storage;
- the supported Home Assistant entity and service surface documented in
  [`docs/PARITY.md`](docs/PARITY.md);
- one shared Blink cloud live session per camera;
- official-compatible Walnut live selected before signaling from a typed
  per-camera transport policy;
- an owner-bound Cayuga/WebRTC signaling implementation retained behind the
  official policy gate for a future production rollout;
- H.264/AAC MPEG-TS for Home Assistant and SceneTrove;
- cached Blink JPEG snapshots;
- fixed-duration local live recordings with immutable SHA-256 manifests;
- server-paginated Sync Module USB inventory, authenticated clip playback/download,
  exact clip deletion and compatible-media formatting;
- native HA camera entities attached to the Vistoda Blink provider device;
- redacted, model-aware camera settings with optimistic concurrency,
  read-back verification and rollback attempts;
- native v1 activity/privacy-zone editing on verified camera generations;
- stable device-ID aliases across provider-side camera renames;
- 75-second battery-camera and 600-second powered-camera session limits;
- bounded subscriber queues and a 4 MiB packet ceiling;
- Bearer or Basic authentication for approved LAN consumers;
- no dependency on `blinkpy`, duplicate runtime session or public listener.

## Private API

The API remains mounted below `/api/blink_live_bridge`:

| Endpoint | Purpose |
| --- | --- |
| `GET /healthz` | readiness and non-secret camera count |
| `GET /v1/cameras` | stable aliases and power class |
| `GET /v1/cameras/{alias}/snapshot.jpg` | cached Blink JPEG |
| `GET /v1/cameras/{alias}/live.ts` | bounded MPEG-TS stream |
| `GET /v1/cameras/{alias}/live.mpegts` | explicit MPEG-TS alias |
| `GET /v1/cameras/{alias}/settings` | typed, redacted settings and revision |
| `POST /v1/cameras/{alias}/settings` | one validated setting with read-back; explicit paired initialization for unset temperature thresholds |
| `GET /v1/cameras/{alias}/capabilities` | bounded field schema and safe feature probes |
| `GET /v1/cameras/{alias}/zone-capabilities` | value-redacted activity/privacy zone schema |
| `GET /v1/cameras/{alias}/zones` | normalized 20×15 activity/privacy zone state |
| `POST /v1/cameras/{alias}/zones` | atomic zone update with revision, verification and rollback |
| `GET /v1/recordings?page=&page_size=&camera=` | server-paginated standalone recording inventory |
| `POST /v1/cameras/{alias}/recordings` | bounded live capture; requires request ID |
| `GET /v1/recordings/{id}/media` | immutable local MPEG-TS media |
| `DELETE /v1/recordings/{id}` | remove a completed local recording |
| `GET /v1/local-storage?page=&page_size=` | server-paginated Sync Module USB inventory and available percentage |
| `GET /v1/local-storage/{network}/{sync}/{manifest}/{clip}/media` | one USB clip without mutation |
| `DELETE /v1/local-storage/{network}/{sync}/{manifest}/{clip}` | delete one revalidated exact USB clip |
| `POST /v1/local-storage/{network}/{sync}/format` | format only provider-declared compatible media |
| `POST /v1/cameras/{alias}/audio/probe` | authenticate native signaling without starting media |
| `GET /v1/cameras/{alias}/webrtc` | gated Cayuga WebSocket signaling; media stays browser-to-Blink |

Core loopback is trusted so HA camera state never contains credentials. Other
clients must send a dedicated high-entropy token. Keep the endpoint private;
do not publish it through Traefik.

## Installation

[![Install Vistoda Blink through HACS](https://my.home-assistant.io/badges/hacs_repository.svg)](https://my.home-assistant.io/redirect/hacs_repository/?owner=luigibarretta&repository=vistoda-blink&category=integration)

Follow the shared [English setup guide](https://github.com/luigibarretta/vistoda-addons/blob/main/GETTING_STARTED.md)
or [guida italiana](https://github.com/luigibarretta/vistoda-addons/blob/main/GETTING_STARTED.it.md).
In short: install both **Vistoda** and **Vistoda Blink** through HACS, restart
Home Assistant, add the **Vistoda Apps** repository, then install and start the
matching Blink app. Supervisor discovery connects the adapter without YAML, a
bridge URL or a user-managed token. Complete login/2FA in the discovered flow.

Existing YAML-token installations remain supported and are migrated without
changing the key used to seal the provider session.
You may remove or disable the official Blink integration after parity has been
verified; normal Vistoda operation never reads it.
The powered Blink Mini is the only automatic production media canary; battery
cameras are never opened by CI or routine deployment checks.

## Recovery and compatibility

Account reconnection, updates, rollback, restore and uninstall are documented in
the shared [operations guide](https://github.com/luigibarretta/vistoda-addons/blob/main/OPERATIONS.md).
The [compatibility matrix](https://github.com/luigibarretta/vistoda-addons/blob/main/COMPATIBILITY.md)
defines the tested component versions and provider boundaries.
Published images include licenses and notices under `/usr/share/doc/vistoda`.
Only exact version tags passing quality, security and provenance gates are released.

## Development

Read the family [contribution guide](https://github.com/luigibarretta/vistoda-home-assistant/blob/main/CONTRIBUTING.md)
first to understand repository ownership and cross-repository release order.

```bash
python -m pip install ".[dev]"
python -m ruff format --check .
python -m ruff check .
python -m compileall -q custom_components tests scripts
python scripts/check_loc.py
python -m pytest
cargo fmt --manifest-path addon/vistoda_blink_engine/Cargo.toml --check
cargo clippy --manifest-path addon/vistoda_blink_engine/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path addon/vistoda_blink_engine/Cargo.toml
```

Tests are deterministic and require no Blink account, network or secret.
Every maintained source, configuration and documentation file is limited to
250 physical lines.
The supervised app bootstrap is vendored from
[`lib-vistoda-provider-kit`](https://git.luigibarretta.com/luigibarretta/lib-vistoda-provider-kit)
at the commit in `dependencies/vistoda-provider-kit.sha` and verified byte-for-byte in CI.

Architectural decisions are indexed in [`docs/adr/`](docs/adr/README.md).
The versioned parity matrix is in [`docs/PARITY.md`](docs/PARITY.md).
The official-app replacement feasibility review is in
[`docs/CAMERA_SETTINGS_AUDIT.md`](docs/CAMERA_SETTINGS_AUDIT.md).
Standalone recording and vendor-storage boundaries are recorded in
[`ADR-0005`](docs/adr/0005-standalone-recordings-and-vendor-media.md).
The native signaling discovery boundary is recorded in
[`ADR-0006`](docs/adr/0006-native-audio-signaling-discovery.md).
Guarded provider-owned USB mutations are recorded in
[`ADR-0007`](docs/adr/0007-guarded-usb-management.md).

## Author, support and independence

Vistoda Blink is maintained by [Luigi Barretta](https://github.com/luigibarretta).
[Support the project on Ko-fi](https://ko-fi.com/luigibarretta). Vistoda is an
independent project; read the shared [disclaimer](https://github.com/luigibarretta/vistoda-home-assistant/blob/main/DISCLAIMER.md)
and [accessibility statement](https://github.com/luigibarretta/vistoda-home-assistant/blob/main/ACCESSIBILITY.md).

Licensed under the MIT License.
