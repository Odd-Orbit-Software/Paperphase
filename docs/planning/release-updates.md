# UI updates from GitHub releases

Status: implemented on `release-updates`, after rotation commit
`5145b19`. Owner-approved requirements remain in requirements.md and delivery-plan.md.
The repository had no published releases at preflight. The owner subsequently
authorized publishing `v0.1.0-foundation.1`; its 24 audited public assets were
published on 2026-11-21 from source commit `2f92ef7`. The owner-requested centered
follow-up `v0.1.0-foundation.2` was published from `e734554`.

Desktop and isolated Pi uploaded-package activation/rollback passed; the regular
managed service restored owner revision 5. Real Pi discovery now finds the first
published releases only after experimental opt-in. Real GitHub app and weather
plugin activation passed through the UI, with backups, acknowledged Updating,
healthy restart and final restoration of owner revision 5. The owner confirmed
the centered message and correct weather display. Both release-commit branch/tag
checks passed; version remains `0.1.0`. See the design validation ledger.

## Delivery contract

- Device → Updates is the ordinary owner flow: check, review release/version,
  explicitly update, see progress, reconnect. Application and installed plugins
  use the same flow. Package upload and a GitHub release URL remain available.
  The owner confirmed normal releases by default and experimental releases opt-in.
- Use signed portable packages with explicit kind, publisher/repository, release
  tag, build identity, platform/ABI/schema compatibility and complete file hashes.
  Verify publisher signature, GitHub asset digest when supplied, byte limits,
  file paths and payload hashes before preparing activation. GitHub HTTPS and a
  downloaded checksum alone are not a publisher-signature check.
- A small independent update agent owns activation and recovery. Its immutable
  executable is outside the app release slot, so a failed replacement cannot
  remove its own rollback process. A supervised application child can restart
  without granting the LAN app arbitrary service-control commands.
- Persist a transaction record before side effects. Fully download/verify first,
  show and acknowledge Updating through the serialized display worker, quiesce
  writes, back up private state, atomically select the prepared release, start,
  health-check, then release normal output. Respect physical cooldowns throughout.
- Preserve layouts, named designs, Google/connection credentials, snapshots and
  provider-rate metadata. Initial packages retain schema/ABI 1; incompatible
  migrations are rejected rather than guessed. Rollback restores the previous
  release and its compatible private state. Interrupted activation is recovered
  by the agent at startup. Failed rollback stays visible and has an SSH path.
- Installed plugin updates keep host-owned origin/auth/reference grants. They
  replace portable Wasm and declared assets, never publisher-provided native
  caches. Plugin identities and host ABI must match the installed target.

## Package and installation direction

Use a bounded signed JSON envelope followed by a gzip stream of ordered regular
file payloads. The signed manifest names every file, exact size and SHA-256.
No archive extraction, symlinks, scripts or arbitrary lifecycle hooks are needed.
Ed25519 verification uses the existing ring dependency. Publisher private keys
stay outside source/images; installations retain only trusted public keys.

The update agent supervises the app and watches a private filesystem queue.
Immutable releases and plugin slots live separately from the private data
folder. Application health requires the expected launch transaction and a
successfully initialized editor/runtime. Normal panel output is held until the
agent approves health. Agent/bootstrap upgrades retain a documented SSH path.

## Validation

Exercise malformed/tampered packages, wrong architecture/ABI, bounded downloads,
path traversal, signature/hash failure, concurrent requests, cancelled browsers,
failed startup, interrupted activation and rollback, preserved private state,
quiet-hours restoration, Updating acknowledgement and real panel restart.
Desktop mock releases, uploaded packages, real GitHub discovery, Pi execution
and human optical checks must be reported separately.

## UI direction

Operate mode; ordinary extension of DESIGN.md. Preserve existing paper/green
surfaces, native controls, Device navigation and separate advanced settings.
Lead with installed software, available release and the next clear action.
Progress names the current action; failures identify outcome and recovery.
Use actual screenshots and a fresh Impeccable finish reviewer before completion.

## Direction contract

THESIS: the owner can identify installed software, review a selected release and
understand the restart/recovery outcome before and after starting an update.

OWN-WORLD: inherit DESIGN.md's paper/green surfaces, native controls and quiet
flat rows. This extends Device and introduces no replacement visual world.

STORY: check releases, review version/notes, explicitly confirm, verify download,
acknowledge Updating, restart and health-check, then resume or recover.

FIRST VIEWPORT: show installed Paperphase and Check for updates immediately;
show the current update outcome/progress above software rows. Starting from a
lower plugin reveals/focuses progress once, without moving the page on each poll.
Keep unrelated layout-editing status out of this view.

FORM: existing Device subsection navigation and a flat software list. Keep
experimental releases, direct URLs and package upload in Advanced. Stack
controls on narrow screens; desktop remains the editing priority.

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance.
