# ADR 0010: Versioned camera-settings backups

- Status: accepted
- Date: 2026-09-13

## Context

Camera settings are independent provider writes. Replaying a stale snapshot by
alias could target a renamed or replaced camera, and a partial restore could
leave an unknown mixture of old and new values. Uninitialized temperature
thresholds have no proven inverse provider operation.

## Decision

Home Assistant retains at most 25 named, redacted snapshots. Each camera is
matched by provider serial and then stable provider ID; an alias alone is not
accepted when a stable identity is present. The backend supports an optional
camera subset for future clients, while the current panel creates and restores
the whole enrolled set.

Before the first write, restore captures a separate rollback snapshot and
preflights every camera and field. It refuses missing or non-writable fields,
identity ambiguity and any changed threshold whose source or target is
uninitialized. Each write carries the last confirmed revision. Final read-back
must reproduce every saved value. Rollback also uses the last confirmed
revision and never retries through a concurrent provider change.

## Consequences

- Multiple recovery points remain selectable without storing credentials.
- Restore fails closed instead of silently skipping unsupported values.
- Concurrent official-app or provider changes win rather than being overwritten.
- A rollback backup is evidence and a recovery option, not a guarantee after an
  ambiguous network failure.
