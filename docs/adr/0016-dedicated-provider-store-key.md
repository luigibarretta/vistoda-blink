# ADR 0016: Dedicated provider-store key and shared secret-file primitives

- Status: accepted
- Date: 2026-11-09

## Context

`provider.sealed` (Blink refresh token, hardware id) was encrypted with
AES-256-GCM under `SHA-256("vistoda-blink-provider-store-v1\0" || workload
token)`. The workload token is the engine's API bearer and is published to the
Home Assistant adapter through discovery, so every holder of the bearer could
also decrypt the provider credentials. Private files were written with
`File::create` plus rename, relying on the process umask for their mode.

## Decision

- The engine depends on `lib-auth-modules` 0.2.0 with only the `keys` and
  `secret-files` features, vendored inside the Docker build context
  (`addon/vistoda_blink_engine/vendor/auth-modules`, pin, tree digest and the
  `auth-modules-vendor` step); public CI never clones the private repository.
- The store key is `MasterKey::derive("vistoda-blink/provider-store/v1")` of a
  dedicated 32-byte random `/data/store.key`, created once with
  `write_private_atomic` (0600, fsync, atomic rename) and read with
  `read_private_file` (owner-only, no symlink, identity-checked). A sealed store
  whose key file is missing fails closed; the key is never regenerated for it.
- Envelope version 2 marks the new key. A version 1 envelope is opened with the
  legacy token-derived key and immediately re-sealed as version 2; if that
  write fails, the credentials are still used and migration is retried on the
  next load.
- `provider.sealed` and `camera-aliases.json` are written with
  `write_private_atomic`; `run.sh` re-secures `store.key` like the store.

## Consequences

Disclosure of the workload token no longer discloses the Blink credentials.
`/data` as a whole (or a full backup) still contains both the key and the
sealed store; that boundary is unchanged and documented in `SECURITY.md`.
Restoring `provider.sealed` without its `store.key` requires reconnecting the
Blink account: the engine logs that it could not restore the provider and
waits; delete `/data/provider.sealed` and enrol again. The first start after
the upgrade rewrites a legacy envelope as v2, so downgrading to an engine
without `store.key` support also requires enrolling again.

## Verification

`credentials_tests.rs` covers the round trip, binding to `store.key` rather
than the token, fail-closed loading without the key, the legacy-to-v2
migration and unknown versions; `store_key.rs` tests the 0600 creation and the
rejection of shared, placeholder and short key files.
