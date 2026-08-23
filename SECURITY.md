# Security policy

Report vulnerabilities privately to the repository owner. Do not open a public
issue containing credentials, camera serials, signed URLs, packet captures or
media.

## Supported release

Only the current `main` revision and the exact SHA deployed by the production
Ansible inventory are supported.

## Security boundary

- Blink credentials and refresh tokens remain owned by Home Assistant's
  official Blink integration.
- The private workload token must be stored outside Git and compared in
  constant time.
- The connector must remain on the trusted Home Assistant listener; it must not
  receive an independent public route.
- Camera sessions, packets, queues and client lifetimes must stay bounded.
- Tests and fixtures must never contain real account or device material.

Rotate the workload token and restart Home Assistant if it may have been
disclosed. Revoke the official Blink session through the vendor-supported flow
if the Home Assistant credential boundary may have been compromised.
