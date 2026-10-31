# Vistoda Blink

La procedura completa è nella [guida italiana](https://github.com/luigibarretta/vistoda-addons/blob/main/GETTING_STARTED.it.md);
the complete procedure is in the [English setup guide](https://github.com/luigibarretta/vistoda-addons/blob/main/GETTING_STARTED.md).

Install and start the app, then complete the automatically discovered
**Vistoda Blink** flow under Settings → Devices & services. Enter the email and
password used by the Blink app and, when requested, the newest verification
code sent by Blink.

The official Blink integration is not required. Vistoda owns one independent
rotating session and exposes native Home Assistant entities and services.

The Vistoda Blink panel uses Blink's supported Walnut/IMMI live transport.
Multiple viewers can share video; only one viewer can hold the Walnut microphone
lease at a time. Simultaneous listening while talking is enabled only when the
actual provider offer and browser/camera echo cancellation support it. Cayuga
remains disabled by Blink's current provider policy.
If a verification code is rejected or expires, enter the account credentials
again to request a fresh challenge, then use only the newest code.

Vistoda reads the real provider state for each supported camera setting. On
verified v1 cameras it also manages the native 20×15 activity grid and up to
two privacy zones with revision checks, provider read-back and rollback. Zone
controls stay unavailable on Owl/Mini v2 devices while Blink rejects the
enrolled route; no empty or guessed configuration is displayed.

The system card can retain up to 25 named settings backups for all enrolled
cameras. Restore matches stable provider serials/IDs, creates a rollback
snapshot, preflights every field and fails on a concurrent revision or an
uninitialized threshold that cannot be reversed.

Vistoda can also record finite 15, 30 or 60 second clips from the shared live
stream. Its standalone archive is distinct from Blink cloud clips and Sync
Module USB storage. Download, confirmed deletion and verified NFS backup are
managed from the Vistoda camera page through Home Assistant. The Sync Module
USB archive has a separate paginated list with playback, download and
checksum-verified NFS copy. Administrators may delete selected exact clips or
format compatible media only after the explicit destructive confirmation.
The panel reports Sync Module firmware, status and storage used. Administrators
can safely eject the USB drive and mount it again from the panel; this path has
not yet been verified on a live drive. Wi-Fi migration and Sync Module removal
remain unavailable.

The app API remains private to the Supervisor network. Do not publish port
8099 unless an advanced external consumer has a separately reviewed need.
