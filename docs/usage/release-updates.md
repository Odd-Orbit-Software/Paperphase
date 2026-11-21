# Update Paperphase and plugins

Open **Device → Updates → Check for updates**. Select a release, read its notes,
then choose **Update** and confirm **Update and restart**. Save/discard your
layout draft first, including changes in other open tabs. Updates are manual;
checking does not install anything. Experimental pre-releases are excluded
unless enabled under **Advanced update options**.

Paperphase downloads the selected package, verifies the trusted publisher,
platform/ABI and all files, then shows **Updating Paperphase** on the e-paper.
It waits for acknowledgement before stopping. The supervisor backs up private
state, activates the release, restarts and health-checks the app. This page
follows read-only supervisor progress while the main app restarts. Keep power
connected. Display cooldowns can delay the Updating notice and restored content.

Explicit updates may show their notice during quiet hours. Normal content after
health approval follows your saved quiet-hours and display policy. Layouts,
connections, Google credentials, reusable designs, cached data and provider-rate
state are preserved. Updating a plugin keeps the host's source/auth/reference
grants and does not reset its polling cooldown. Failed verification leaves the
installed version untouched; failed startup triggers rollback with compatible
saved data. If rollback fails, use the recovery steps below.

Advanced controls also accept a specific release page from the configured GitHub
repository, or a downloaded signed `.ppkg` file. Select the matching installed
software first. Packages cannot run arbitrary install scripts, change host
permissions or import a publisher's native Wasm cache. This first updater keeps
state schema 1, updater protocol 1 and `paperphase_v0`; incompatible migrations
are rejected. Native Linux GNU aarch64 and x86_64 builds require a compatible
loader; the capability preflight rejects an executable that cannot run here.

## One-time setup for existing development images

Factory images should include this setup. Existing manually deployed images
need the supervisor installed once through SSH. This is an operator step; normal
owners then use the UI. The examples match the current test installation; adapt
usernames and paths for another image.

Build both native binaries and the portable plugins using the documented host
and cross-compilation setup. The supervisor executable lives in a bootstrap
folder outside writable app release slots. Keep it there across core updates.
The config contains only trusted **public** signing keys, target identities,
paths and serve arguments. Do not copy publisher private keys to an image.

Create `bootstrap/` and `managed/` as private directories first. With the new app
at `bin/paperphase`, supervisor at `bootstrap/paperphase-update-agent`, and the
existing `live/` data preserved:

```sh
python3 scripts/configure-updates.py \
  --output /home/pmck/paperphase-test/installation.json \
  --root /home/pmck/paperphase-test/managed \
  --data-dir /home/pmck/paperphase-test/live \
  --application /home/pmck/paperphase-test/bin/paperphase \
  --working-dir /home/pmck/paperphase-test \
  --platform aarch64-unknown-linux-gnu \
  --bindings /home/pmck/paperphase-test/live/bindings.json \
  --health-url http://127.0.0.1:8080/api/update-health \
  --status-port 8082 \
  -- serve --listen 0.0.0.0:8080 --state-file live/workspace.json \
  --connector-bindings live/bindings.json --display waveshare-v2-partial \
  --waveshare-vendor-dir /home/pmck/paperphase-test/panel
```

The script uses the repository's public publisher keys by default and refuses
to overwrite an existing config or put managed releases inside the data backup.
Custom repositories require explicit `--publisher KEY_ID=BASE64_PUBLIC_KEY`.
It inventories installed modules without copying source credentials or changing
host grants/private state. Review the generated configuration before installing
[the managed service](../../deploy/paperphase-managed.service) as
`/etc/systemd/system/paperphase.service`. Stop the old service before replacing
its binary/unit, keep its binary/unit backup, reload systemd and start the new
service. Preserve the existing physical journal's cooldown throughout.

Atomic data-folder rollback needs write access to the data folder's parent.
The managed unit therefore permits the installation parent and marks bootstrap,
baseline binaries, vendor files and config read-only within it. Keep these nested
protections when adapting paths; allowing only `live/` prevents atomic directory
replacement. This follows systemd's [path namespace controls](https://www.freedesktop.org/software/systemd/man/latest/systemd.exec.html#ReadWritePaths=).

Port 8082 is read-only supervisor progress. It has no activation/configuration
API and returns no credentials or private paths. Browser reads are restricted
to the same hostname and configured app port. The main editor permits this
specific status origin in its CSP. Set `status_port` to 0 to disable it; browser
progress then waits for the main app to reconnect. Keep normal LAN access rules.

The supervisor's health check expects `/api/update-health` on loopback and the
new child's launch identity. It waits up to 180 seconds with a short stable
ready period. Normal output stays held until health approval. The independent
supervisor is not replaced by a core UI update; bootstrap upgrades retain the
SSH path.

## SSH status and recovery

```sh
/home/pmck/paperphase-test/bootstrap/paperphase-update-agent status \
  --installation /home/pmck/paperphase-test/installation.json
```

If automatic rollback reports **Recovery needed**, stop the managed service,
restore the recorded previous release/data, then restart:

```sh
sudo systemctl stop paperphase.service
/home/pmck/paperphase-test/bootstrap/paperphase-update-agent recover \
  --installation /home/pmck/paperphase-test/installation.json
sudo systemctl start paperphase.service
```

Recovery refuses to race a running supervisor. It uses the retained complete
backup and previous release record, preserves a conservative physical cooldown,
and retains failed-version data in a private sibling folder for diagnosis.
An interrupted backup never replaces the original data. If the backup or former
executable is unavailable, leave the retained files in place and recover using
the known-good bootstrap binary/unit and private-state backup. Do not delete a
backup while its transaction needs recovery.

This foundation build retains completed transaction records, backups and release
slots; automatic retention cleanup is not implemented yet. Check available disk
space when maintaining the installation. Retained recovery files are private.

## Publishing signed releases

The repository provides `deploy/publisher-keys.json`; it contains public keys
only. Publisher private keys stay outside source and images. The current local
signing key is stored in the ignored `.paperphase/release-signing/` folder;
back it up privately before discarding this workspace. It is not a release asset.

To create a separate publisher key at an explicit private path:

```sh
paperphase-update-agent keygen --private-key /private/path/publisher.pk8
```

The command prints only its public key. Keep the key identifier/public value in
the installation trust configuration. `public-key --private-key PATH` prints
that public value again. Private `.pk8` files and `.ppkg` build artifacts are
ignored by Git; the package builder includes only declared payloads/notices.

Create signed packages using a **native** signing tool, even when packaging an
ARM application built on another architecture:

```sh
python3 scripts/package-release.py \
  --agent target/release/paperphase-update-agent \
  --private-key /private/path/publisher.pk8 --key-id paperphase-release-1 \
  --release v0.1.0-foundation.1 --build SOURCE_COMMIT \
  --output .paperphase/release-candidate \
  --platform aarch64-unknown-linux-gnu \
  --application target/aarch64-unknown-linux-gnu/release/paperphase \
  --plugin weather.open-meteo=target/wasm32-unknown-unknown/release/paperphase_weather_plugin.wasm \
  --icons weather.open-meteo=plugins/weather/assets
```

Add `--plugin CONNECTOR=MODULE` for each other compiled plugin. Include the
matching architecture's bootstrap supervisor separately for one-time installs.
The script writes `.ppkg`, public manifest sidecars and `SHA256SUMS`, and refuses
to overwrite an existing release asset. Upload the packages to the matching
GitHub release. Discovery expects `paperphase-<Rust-target>.ppkg` for the core
and `<connector-id>.ppkg` for each plugin. Do not upload private keys, configs,
backups, live data or local Wasm native caches.

Foundation package/application version remains `0.1.0`; release tags/build IDs
distinguish experimental builds. Mark foundation releases as pre-releases so
normal owners must opt in. Publishing is a separate explicit maintainer action;
no release is created just by checking, uploading a local package, or committing.
GitHub asset SHA-256 adds transfer integrity; the Ed25519 publisher signature is
checked separately against installed public keys. See the
[GitHub release asset API](https://docs.github.com/en/rest/releases/assets).

## Validation status

Desktop package/process/browser checks, real GitHub discovery, uploaded-package
activation, Pi execution and optical checks are separate evidence. Current
results are recorded in [the update design](../design/release-updates.md) and
`changelog.md`; unperformed checks must not be inferred from a working simulator.
