# 0001 — Vistoda product identity with stable HA contracts

Status: accepted, 2026-08-23.

## Context

The connector began as `Blink Live Bridge` embedded in the homelab Ansible
repository. Vistoda later became the common product identity for Blink, EZVIZ,
Ring and the Home Assistant control plane. Keeping product code inside the
infrastructure repository obscured ownership and prevented independent release
and testing.

Home Assistant config entries, camera entity IDs, automations and SceneTrove
already depend on the `blink_live_bridge` domain and private API path.

## Decision

The product repository and user-facing integration are named **Vistoda Blink**.
The source moves to the standalone `vistoda-blink` repository with its history,
tests, CI, license and release metadata.

The following compatibility identifiers remain stable:

- Home Assistant domain `blink_live_bridge`;
- API prefix `/api/blink_live_bridge`;
- Vistoda provider identifier `blink:blink`;
- existing camera unique IDs and aliases;
- the single official Blink coordinator as vendor-session owner.

Vistoda for Home Assistant adopts the connector's existing provider device. It
does not duplicate cameras or authenticate to Blink again.

## Consequences

Repository and product ownership are clear without a registry migration or
consumer cutover. Ansible pins and deploys an exact clean repository revision
instead of embedding application code. The technical domain retains its legacy
wording until a future Home Assistant-supported migration can prove preservation
of config entries and entity identities.
