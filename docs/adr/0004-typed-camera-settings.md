# ADR 0004: Typed camera settings with read-back and restoration

- Status: Accepted
- Date: 2026-09-09

## Context

Blink camera generations return different configuration shapes and support
different subsets of the official application's settings. A generic JSON proxy
would expose provider internals and could accept a valid-looking value for the
wrong device generation. An HTTP success alone does not prove that a camera
committed a cloud setting.

## Decision

The Rust provider owns configuration discovery and mutation. It returns only a
fixed semantic allowlist with explicit kind, current value, writable flag,
range or enum, and a SHA-256 revision over that redacted view. Unknown fields
remain invisible.

An update accepts one semantic key and value. The provider serializes writes,
requires the previous revision, validates the model-derived field contract and
sends only the mapped vendor key. It then rereads the configuration. If the
value does not match, it sends the prior vendor value and verifies restoration
before reporting failure.

Home Assistant exposes the read through its authenticated WebSocket and limits
writes to administrators. Neither the browser nor the HA adapter receives Blink
credentials or raw configuration.

## Consequences

- Supported settings expand per returned capability instead of product-wide
  assumptions.
- Concurrent stale edits fail with a conflict instead of overwriting a newer
  value.
- Settings without proven units, ranges, paths or rollback stay read-only or
  hidden.
- Verification failure is visible even when the provider accepted the request.
