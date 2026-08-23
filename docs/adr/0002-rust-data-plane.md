# 0002 — Standalone Rust provider with a minimal HA adapter

Status: accepted, 2026-08-23.

## Context

Vistoda Blink began as a pure Python extension of Home Assistant's official
Blink coordinator. That made live media quick to deliver but made Vistoda
unavailable whenever the official integration was absent, unloaded or changed
its private runtime objects. The product requirement is stricter: Vistoda Blink
must operate independently and provide the complete Home Assistant Blink
surface plus live media.

Home Assistant custom integrations must also retain a Python entry point. The
media parser, queues and fan-out do not share that constraint and benefit from
Rust's bounded memory, ownership and process isolation.

## Decision

Vistoda Blink becomes a standalone Rust provider in release 0.3.0:

- the supervised `vistoda_blink_engine` HA app owns OAuth2 PKCE enrollment,
  2FA, refresh tokens, discovery, polling, commands, media downloads, live IMMI
  transport, MPEG-TS validation, fan-out, backpressure and metrics in Rust;
- a minimal Python adapter handles only Home Assistant config flow, entities,
  service registration and authenticated proxying to the private Rust API;
- neither runtime imports `blinkpy` nor reads the official Blink coordinator;
- an explicit one-time migration endpoint may import a refresh token from a
  loaded official integration, but normal operation never calls it;
- direct username/password/2FA enrollment remains available when no official
  integration exists;
- no engine port is published to the host or LAN, and the existing 64-hex
  workload token authenticates both internal and consumer traffic;
- the existing domain, API URLs, camera IDs and Vistoda provider identifier
  remain unchanged.

## Consequences

The provider can replace the official HA integration and can also coexist with
it during a measured migration. Refresh credentials are sealed at rest with a
key derived from the independently managed workload token; passwords and 2FA
codes are transient. A few small Python files remain because Home Assistant
custom integrations are Python entry points, but no Blink protocol code does.
Deployment independently pins and content-compares the HA app and adapter before
restarting Core only for changed Python. Rollback material for both components
remains under the dedicated Home Assistant backup directory.
