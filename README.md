# Blink Live Bridge

This Home Assistant custom integration adds on-demand MPEG-TS live view without
creating another Blink login. It reuses the single loaded Core `blink` config
entry and exposes the same stream to native HA camera entities and private
workload clients such as SceneTrove.

After loading, it initiates Home Assistant integration discovery for Vistoda.
Accepting that discovery adds only the Vistoda provider device: the existing
Blink live cameras, private API and single vendor session remain authoritative.
The camera entities claim the same device-registry identifier as the Vistoda
Blink provider, so Home Assistant displays them under `Vistoda · BLINK` without
creating duplicate entities or vendor sessions.

## Private API

The API is mounted below `/api/blink_live_bridge`:

- `GET /healthz` reports readiness and camera count;
- `GET /v1/cameras` lists stable, non-secret aliases and power class;
- `GET /v1/cameras/{alias}/snapshot.jpg` returns the Blink cached JPEG;
- `GET /v1/cameras/{alias}/live.ts` streams MPEG-TS;
- `GET /v1/cameras/{alias}/live.mpegts` is an equivalent explicit alias.

Loopback requests from Core are trusted so the HA stream worker never embeds a
credential in entity state or logs. LAN clients must send either
`Authorization: Bearer TOKEN` or HTTP Basic with the token as password. The
token is derived by Ansible from the vaulted HA credential with a
domain-separated SHA-256 expression and is stored only in HA `secrets.yaml`.

SceneTrove should use a private base URL such as
`http://it1-prd-iot-01:8123/api/blink_live_bridge/` and the workload token file.
Its connector must select MPEG-TS for Blink; the existing browser-facing
fragmented-MP4 packaging remains a SceneTrove responsibility.

## Safety and lifecycle

- One cloud live session is shared by all local subscribers to the same camera.
- Powered `owl`/Mini cameras have a 600-second upper bound.
- Battery cameras have a 75-second upper bound.
- Slow subscribers use bounded queues; old packets are dropped instead of
  growing Home Assistant memory.
- The final subscriber disconnect cancels keepalive/polling and ACKs the Blink
  command.
- IMMI headers and payloads use `readexactly`; fragmented TCP reads cannot
  truncate a packet.
- Payloads over 4 MiB are rejected.

The deploy and canary are intentionally separate. A media-canary failure keeps
the healthy loaded candidate for inspection and never triggers an automatic
second Core restart.

## Verification

Run locally:

```bash
python3 -m unittest tests.test_blink_live_bridge_protocol
ansible-lint playbooks/deploy-ha-blink-live-bridge.yml \
  playbooks/reconcile-ha-blink-live-bridge.yml
```

Production uses `camera.kitchen_camera_live` as the powered canary and requires
a non-empty bounded HLS media segment. Battery cameras are not opened by the
automatic canary.
