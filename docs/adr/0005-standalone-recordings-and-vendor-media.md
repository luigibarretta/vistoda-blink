# ADR 0005: Standalone recordings and vendor-media boundary

- Status: accepted
- Date: 2026-09-09

## Context

The former “record clip” action called a Blink cloud command. On this account it
raised a false motion notification and returned a provider error, while the user
expected a recording of the live view. Blink Sync Modules may also contain USB
media, but that is a different vendor-owned archive.

Android 59.1 proves endpoints for manifest requests, local-storage media,
clip deletion, mount/eject and format. It does not by itself prove the enrolled
module's asynchronous command payloads, media pagination or safe recovery after
an interrupted destructive operation.

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

The Sync Module USB archive remains hidden until a live read-only manifest
canary proves the exact response and polling contract. Eject, mount, format and
vendor deletion require separate typed confirmations and rollback evidence.

## Consequences

- Recording the live view no longer creates a Blink motion event.
- The archive works independently of a Blink subscription or Sync Module USB.
- Fixed duration is explicit; arbitrary stop is deferred because battery camera
  session lifetime and interrupted-browser ownership need a durable cancel API.
- Vistoda does not claim to replace the official app for USB administration or
  two-way talk until those protocols are independently verified.
