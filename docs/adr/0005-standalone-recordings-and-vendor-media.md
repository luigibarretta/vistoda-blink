# ADR 0005: Standalone recordings and vendor-media boundary

- Status: accepted
- Date: 2026-09-09

## Context

The former “record clip” action called a Blink cloud command. On this account it
raised a false motion notification and returned a provider error, while the user
expected a recording of the live view. Blink Sync Modules may also contain USB
media, but that is a different vendor-owned archive.

Android 59.1 proves endpoints for manifest requests, local-storage media,
clip deletion, mount/eject and format. The read-only status, manifest request,
command polling, media index and clip request form a separable contract; no
destructive operation is needed to enumerate or download media.

## Decision

The engine records 15, 30 or 60 seconds from its existing per-camera live hub.
One camera has at most one active capture. Files are created privately, bounded
to 96 MiB, fsynced and atomically published as MPEG-TS. A persisted manifest
contains timestamps, byte count, SHA-256 and failure state. Startup removes
partials and marks interrupted jobs failed. The spool has a 512 MiB ceiling.

Home Assistant exposes admin-only create/delete operations, authenticated or
signed media download and the ordinary user-readable inventory. The Vistoda
control plane may copy ready media to a separately mounted, checksum-verified
NFS archive. Deleting local media never deletes its NFS copy.

Android 59.1 also shows that saving an active Walnut live is a session command,
not a direct file upload: `saveLive(true)` submits `SaveClip`, while
`saveLive(false)` submits `DiscardClip`. The provider acknowledges saved,
waiting or discarded state. Vistoda implements that bounded command and labels
its destination “Blink archive”: Blink, not Vistoda, selects cloud or the Sync
Module USB according to the account's active Local Storage configuration. This
is the default live-save action; the separate HA capture remains an explicit
alternative. Stopping the live finalizes a provider-marked clip.

The Sync Module USB archive exposes only a bounded status, manifest inventory
and authenticated clip stream. The engine serializes these requests, caps the
inventory at 1,000 entries, returns at most 50 items per page and caps a streamed
clip at 128 MiB. Home Assistant presents ten-item pages and signs playback and
download paths. It may stream a provider-owned clip into the same fail-closed
NFS archive, computing its own SHA-256 before atomic publication. The source is
never acknowledged or modified. Eject, mount, format and vendor deletion are
absent from both engine and UI contracts.
The provider's `enabled` compatibility flag is not used as a readability gate:
like the native app, Vistoda trusts the exact `active` and `memory_full` states.

## Consequences

- HA-local recording does not create a Blink motion event. Provider-managed
  saving intentionally follows Blink's native live-save behavior.
- The archive works independently of a Blink subscription or Sync Module USB.
- HA-local duration is explicit. Provider-managed saving lasts until the live
  ends and can be discarded while the compatible session is still active.
- Vistoda can replace the official app for paginated USB browsing, playback,
  download and NFS copy after live canaries pass; destructive USB administration
  remains official-app-only.
- Two-way talk remains gated until signaling and media are independently verified.
