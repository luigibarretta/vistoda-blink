# ADR 0017: Shared provider kit crate for sealing, private files and bearer checks

- Status: accepted
- Date: 2026-11-11

## Context

ADR 0016 sealed `provider.sealed` under an HKDF sub-key of a dedicated
`store.key`. The EZVIZ and Ring bridges still stored their vendor sessions in
plaintext, and the three bridges each carried their own atomic private
writes and constant-time bearer check. lib-vistoda-provider-kit 0.2.0 now
ships one Rust crate for that code, built on the same lib-auth-modules v0.2.0
primitives.

## Decision

The engine depends on `vistoda-provider-kit`, vendored at
`addon/vistoda_blink_engine/vendor/vistoda-provider-kit` at the kit pin and
verified (bytes and tree digest) by the `provider-kit` step. Its
`../auth-modules` dependency is the existing `vendor/auth-modules` archive,
which is byte-identical to the kit's own copy.

- `credentials.rs` uses `sealed_store::SealedFile` with `KeyFile` label
  `vistoda-blink/provider-store/v1` and the legacy key
  `SHA-256("vistoda-blink-provider-store-v1" || 0x00 || workload token)`:
  the same envelope, key file and derivations as ADR 0016, so existing stores
  open unchanged. The kit's tests and `credentials_tests.rs` open envelopes
  written by 0.25.1 (both versions) from fixtures produced by that code.
- `store_key.rs` is removed; the alias store and recording journals use
  `private_file::write_atomic`/`write_json`; `auth.rs` uses the kit's
  constant-time bearer comparison.
- Sealed files and `store.key` are now read with the private-file checks
  (no symlink, no group/other bits, owned by the engine user), which `run.sh`
  already guarantees through `vistoda_secure_file`.

## Consequences

The sealing format is now a family contract owned by the kit (see its
consumer contract); a change to it lands in the kit first and reaches the
three bridges through their pins. Threat-model limits are unchanged from
ADR 0016.
