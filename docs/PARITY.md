# Blink parity contract

The reference is Home Assistant Core 2026.9.1 with `blinkpy` 0.25.9. A release
cannot claim parity until every row below has a deterministic contract test and
the applicable rows pass a bounded live canary against the enrolled account.

| Surface | Official Blink | Vistoda target | Evidence |
| --- | --- | --- | --- |
| OAuth2 PKCE login | yes | standalone Rust | fixture + live enrollment |
| Email/SMS 2FA | yes | standalone Rust | fixture + enrollment state machine |
| Refresh and reauth | yes | sealed token + native reauth flow | 401 contract + live refresh |
| Scan interval | configurable | configurable | config-flow test |
| Sync Module arm/disarm | alarm panel | alarm panel | API + entity + live command |
| Camera snapshot | cached camera | cached camera | JPEG contract + live refresh |
| Motion control | camera + switch | camera + switch | API + entity + live command |
| Motion detected | binary sensor | binary sensor | state fixture + live state |
| Low battery | binary sensor | binary sensor | state fixture |
| Temperature | °F sensor | °F sensor | state fixture |
| Wi-Fi strength | dBm sensor | dBm sensor | state fixture |
| Record clip | entity service | entity service | command fixture + bounded live call |
| Trigger snapshot | entity service | entity service | command + fresh JPEG canary |
| Save latest video | entity service | entity service | bounded file/hash or exact no-clip no-op |
| Save recent clips | entity service | entity service | bounded files or exact empty result |
| Diagnostics | redacted | redacted | secret-redaction test |
| Device metadata | serial/model/version | equivalent | HA 2026.8+ parent-ID contract test |
| Live video | absent | bounded MPEG-TS | parser/fan-out + powered-camera canary |
| Private consumers | absent | HA + SceneTrove API | auth and stream contract tests |
| Camera settings | official app only | typed model-aware state and writes | unit + reversible live canary |
| Speaker volume | official app | Mini/Owl integer 1–8 | Android 59.1 contract + reversible canary |
| Activity zones | official app | native v1 20×15 grid | model tests + reversible live canary |
| Privacy zones | official app | native v1 spans, maximum two | model tests + reversible live canary |
| Sync Module USB clips | official app | paginated browse/copy + guarded delete/format | Android 59.1 routes; non-destructive live canaries only |
| Blink talk | official app | Walnut selected by current official policy; Cayuga signaling retained but gated | Android 59.1 marks WebRTC `InProgress`; later enabled build + powered-camera SDP/ICE/audio canary |
| Device removal | official app | deliberately deferred | reauth and recovery contract required |

Parity means behavioral coverage, not identical internal implementation. The
legacy `blink_live_bridge` domain and existing live camera unique IDs remain
stable. Replacement entities use Vistoda-owned unique IDs so official and
standalone providers can coexist during cutover without registry corruption.
When Blink exposes no cloud clips, the official integration has no current
video, no last record and an empty recent-clips list; Vistoda preserves that
attribute and service behavior instead of manufacturing a recording.
