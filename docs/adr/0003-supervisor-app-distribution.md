# ADR 0003: Supervisor app distribution

- Status: Accepted
- Date: 2026-08-23

## Context

The Rust engine already ran under Supervisor, but users still had to create a
workload token, place it in YAML and install the adapter separately. That
exposed an internal authentication detail and made Blink unlike other Vistoda
providers.

## Decision

The app generates or migrates its workload token in `/data`, keeps port 8099
private and publishes URL/token data through a `blink_live_bridge` Supervisor
discovery message. The HACS adapter accepts `hassio` discovery, stores the
private connection in its config entry and keeps YAML as a compatibility path.

The provider repository publishes one `amd64`/`aarch64` app image. The shared
`vistoda-addons` repository owns store metadata, while this repository remains
the only owner of engine and adapter source.

## Consequences

New users configure only their Blink account and MFA. Existing installations
retain their sealed provider session because the legacy token is migrated
before a new token can be generated. SceneTrove and existing entity/API
identifiers remain unchanged.

