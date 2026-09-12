# Protocol provenance and verification status

This dossier records evidence available at the 2026-09-12 audit. It does not
retroactively establish a clean-room process or certify a legal exception.

| Implemented contract | Recorded evidence | Implementation / checks |
| --- | --- | --- |
| Native signaling message names and fields | Analysis of Blink Android 59.1 DEX schemas recorded on 2026-09-10; ADR 0006 | `blink_webrtc_wire*`, typed protocol fixtures |
| Cayuga/Walnut selection and close 38 fallback | Android 59.1 feature resolver and session-manager analysis; ADR 0008 | `blink_parse.rs`, `blink_signaling.rs`, close-code fixtures |
| Session identity | Authenticated account/device discovery | Private IDs remain inside the provider; credentials are not test fixtures |

The first two rows are not evidence derived solely from open-source libraries.
Do not claim that use of Rust, or publication under an open-source license,
establishes independent origin or permission to access third-party services.

## Missing evidence to resolve before legal clearance

- App artifact hash and acquisition record for the lawfully held copy studied.
- Exact analysis tools/methods and portions examined, with dates.
- Why each unavailable interface detail was necessary for interoperability;
  document attempts to obtain adequate information through available channels.
- Mapping from observations to fields/behavior, without publishing proprietary
  code, credentials, device identifiers or raw user captures.
- Applicable account terms and legal review of the actual facts.

Record unknown items as unknown. Do not replace the historical artifact with a
new download and claim its hash establishes the older analysis. Do not enable
currently gated microphone functionality to fill a documentation gap.

## IMMI TLS hardening

The audited revision accepted server certificates without chain/name/time
validation. Public roots alone do not authenticate the observed IMMI endpoint:
it presents a self-signed, CN-only Blink certificate. The Android native client
requires verification against embedded trust material; account login/OTP is a
separate authentication layer.

The local remediation uses standard WebPKI verification, or an exact SHA-256
certificate identity from the authenticated Android trust material for IMMI.
Pinned certificates are accepted only within their common validity interval and
for an IP address or a single-label `immedia-semi.com` subdomain. TLS handshake
signatures are still verified. There is no accept-all fallback, downloaded trust
update or trust-on-first-use. Unknown/rotated certificates fail closed until a
reviewed release authenticates replacement trust material. This is deliberately
narrower than trusting arbitrary certificates issued by an embedded CA.

The Android base and ARM64 split pass APK v3 signature verification with signer
SHA-256 `ca5b36d12fe84935dd7de849fe9f7fd318b8a413b95ec200b38e73f4fd0cdaf2`.
On 2026-09-12, Blink's HTTPS [Digital Asset Links](https://applinks.blink.com/.well-known/assetlinks.json)
independently associated that fingerprint with `com.immediasemi.android.blink`.
Base APK SHA-256: `77e6fefb8dbbd68f1e964e0822de14d4a5408e9fe5f648eb7d5cce19238e12d7`;
ARM64 split: `acaa8442569c5879c7f7041fa54db6cde1446165aa3683cec7723aff684edca1`.
These hashes identify the current inspection, not historical acquisition.
Only certificate fingerprints and independently authored implementation are
distributed, not the APK, proprietary native code or extracted certificate bundle.

Synthetic tests reject wrong hostnames, unknown issuers and expired certificates.
The production Rust connector also passed a TLS-only handshake against the
observed endpoint, without login, media or device activation. Promotion still
requires a bounded live canary with aligned MPEG-TS and normal teardown. This
research does not establish that Blink's official client has a TLS vulnerability.

## Temperature alert contract

Android 59.1 schema/control-flow inspection identified the camera configuration
fields `temp_alarm_enable`, `temp_min`, `temp_max` and `signals.temp`, the separate
temperature-alert enable/disable action, and the `calibrate` request contract.
Threshold updates require integer Fahrenheit values for both limits and
`current_temp`. Vistoda reads fresh configuration and echoes the current measured
temperature, rather than supplying an invented calibration value. Source paths
examined: `CameraApi`, `TemperatureCalibrationPostBody`,
`DeviceSettingsTemperatureViewModel`, `TemperatureOperatingRange`, `CameraType`.
This is protocol observation, not copied application code or an official API.
