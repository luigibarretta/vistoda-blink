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

The Sync Module USB archive exposes only a bounded status, manifest inventory
and authenticated clip download. The engine serializes these requests, caps the
inventory at 1,000 entries and a downloaded clip at 128 MiB. Home Assistant
shows at most 250 clips and signs each download path for five minutes. Eject,
mount, format and vendor deletion are absent from both engine and UI contracts.

## Consequences

- Recording the live view no longer creates a Blink motion event.
- The archive works independently of a Blink subscription or Sync Module USB.
- Fixed duration is explicit; arbitrary stop is deferred because battery camera
  session lifetime and interrupted-browser ownership need a durable cancel API.
- Vistoda can replace the official app for read-only USB browsing after its
  live canary passes; destructive USB administration remains official-app-only.
- Two-way talk remains gated until signaling and media are independently verified.
