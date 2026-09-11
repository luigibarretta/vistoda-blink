# Security policy

Report vulnerabilities privately to the repository owner. Do not open a public
issue containing credentials, camera serials, signed URLs, packet captures or
media.

## Supported release

The latest tagged release receives security fixes. Development commits are
not release artifacts. Update the provider app and its matching HA adapter.

## Security boundary

- Blink credentials and rotating refresh tokens stay in the standalone Rust
  provider's sealed private data. The official integration is not required.
- The private workload token must be stored outside Git and compared in
  constant time.
- The connector must remain on the trusted Home Assistant listener; it must not
  receive an independent public route.
- Camera sessions, packets, queues and client lifetimes must stay bounded.
- Tests and fixtures must never contain real account or device material.

If the workload token was disclosed, stop the provider, rotate its private
workload-token file, then start the app and reload its discovered HA adapter.
If provider credentials were disclosed, revoke the Vistoda session using the
Blink app and reconnect the account in Home Assistant. Preserve private backups
as sensitive credentials: restoring one also restores its authorization.
