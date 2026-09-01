# Vistoda Blink

Vistoda Blink is a standalone Rust provider that adds complete Blink control,
state and bounded live MPEG-TS media to Home Assistant. It belongs to the
Vistoda product family alongside `vistoda-ezviz`, `vistoda-ring` and
`vistoda-home-assistant`.

The supervised Rust engine owns Blink OAuth2/2FA, token refresh, discovery,
polling, controls and media. The small Python custom integration is only the
native Home Assistant adapter. The official Blink integration is not required;
it can coexist temporarily as a parity oracle or one-time credential migration
source.

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
- complete Home Assistant Blink entity and service parity;
- one shared Blink cloud live session per camera;
- H.264/AAC MPEG-TS for Home Assistant and SceneTrove;
- cached Blink JPEG snapshots;
- native HA camera entities attached to the Vistoda Blink provider device;
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

Core loopback is trusted so HA camera state never contains credentials. Other
clients must send a dedicated high-entropy token. Keep the endpoint private;
do not publish it through Traefik.

## Installation

[![Install Vistoda Blink through HACS](https://my.home-assistant.io/badges/hacs_repository.svg)](https://my.home-assistant.io/redirect/hacs_repository/?owner=luigibarretta&repository=vistoda-blink&category=integration)

Install **Vistoda Blink** through HACS, add the shared **Vistoda Apps**
repository, then install and start the matching Blink app. Supervisor discovery
connects the adapter without YAML, a bridge URL or a user-managed token. Open
the discovered integration and complete login/2FA or an approved one-time
migration.

Existing YAML-token installations remain supported and are migrated without
changing the key used to seal the provider session.
You may remove or disable the official Blink integration after parity has been
verified; normal Vistoda operation never reads it.
The powered Blink Mini is the only automatic production media canary; battery
cameras are never opened by CI or routine deployment checks.

## Development

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
[`vistoda-provider-kit`](https://git.luigibarretta.com/luigibarretta/vistoda-provider-kit)
at the commit in `dependencies/vistoda-provider-kit.sha` and verified byte-for-byte in CI.

Architectural decisions are indexed in [`docs/adr/`](docs/adr/README.md).
The versioned parity matrix is in [`docs/PARITY.md`](docs/PARITY.md).
Licensed under the MIT License.
