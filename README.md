# Vistoda Blink

Vistoda Blink is the Home Assistant-local connector that adds bounded live
MPEG-TS media to an already authenticated Blink integration. It belongs to the
Vistoda product family alongside `vistoda-ezviz`, `vistoda-ring` and
`vistoda-home-assistant`.

The connector deliberately does not create a second Blink login. It reuses the
single loaded Home Assistant Blink coordinator, publishes native live camera
entities and shares the same upstream session with approved private consumers
such as SceneTrove.

## Architecture

```text
Blink cloud
    |
Home Assistant official Blink integration
    |
Vistoda Blink -> native HA cameras
              -> private MPEG-TS/snapshot API -> SceneTrove
    |
Vistoda for Home Assistant adopts the existing provider device
```

The stable Home Assistant domain remains `blink_live_bridge`. Keeping this
technical identifier preserves the existing config entry, camera entity IDs,
private API and Vistoda discovery contract during the product rename.

## Capabilities

- one shared Blink cloud live session per camera;
- H.264/AAC MPEG-TS for Home Assistant and SceneTrove;
- cached Blink JPEG snapshots;
- native HA camera entities attached to the Vistoda Blink provider device;
- 75-second battery-camera and 600-second powered-camera session limits;
- bounded subscriber queues and a 4 MiB packet ceiling;
- Bearer or Basic authentication for approved LAN consumers;
- no second vendor login, duplicate camera or public listener.

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

Production is SHA-pinned and deployed by the homelab Ansible playbook. For a
manual development install, copy `custom_components/blink_live_bridge` into
Home Assistant's `/config/custom_components`, configure the official Blink
integration first and add the required token through `configuration.yaml`:

```yaml
blink_live_bridge:
  token: !secret blink_live_bridge_token
```

Restart Core once, add **Vistoda Blink**, then accept the Vistoda provider
discovery. The powered Blink Mini is the only automatic production media
canary; battery cameras are never opened by CI or routine deployment checks.

## Development

```bash
python -m pip install ".[dev]"
python -m ruff format --check .
python -m ruff check .
python -m compileall -q custom_components tests scripts
python scripts/check_loc.py
python -m pytest
```

Tests are deterministic and require no Blink account, network or secret.
Every maintained source, configuration and documentation file is limited to
250 physical lines.

Architectural decisions are indexed in [`docs/adr/`](docs/adr/README.md).
Licensed under the MIT License.
