# ADR 0007: Guarded Sync Module USB management

- Status: accepted
- Date: 2026-09-10
- Supersedes the read-only mutation decision in ADR 0005

## Context

Android 59.1 proves exact native endpoints for deleting one indexed local
storage clip and formatting compatible Sync Module media. Vistoda already reads
the provider's status, requests the current manifest and pages its contents.
Users need bounded cleanup without allowing stale identifiers or broad support
operations to reach the provider.

## Decision

The engine exposes only exact clip deletion and compatible-media formatting.
Before deletion it revalidates that the network and Sync Module belong to the
current provider state, obtains a new manifest, requires the displayed manifest
to still be current and confirms the clip ID exists. A stale page fails closed.

Before formatting it revalidates the exact network/Sync Module and re-reads
`usb_format_compatible`. Home Assistant requires an administrator and the exact
typed phrase `FORMATTA network/sync`. The Rust boundary repeats that phrase
check. Eject, mount and delete-all routes remain absent.

The status contract interprets Blink's integer `usb_storage_used` as a
percentage only when it is within 0–100 and derives the available percentage.
It does not invent byte capacity.

## Consequences

- A rotated manifest cannot cause deletion of a different clip.
- The UI may offer single and selected deletion while preserving backend pages.
- Formatting is visible only when the provider declares the support compatible.
- Existing provider media and real formatting are excluded from automated and
  manual release tests unless the target was created solely for that test.
