# Configure connections

Start the [desktop editor](desktop-editor.md), then choose **Connections** in the
top bar. The bundled **Demo weather settings** descriptor exercises the form
builder. That descriptor saves settings only. The **Temperature connector** can
[fetch through a registered compiled module](compiled-connectors.md).

1. Select **New connection**, choose a descriptor, and name the record.
2. Set enabled state and polling interval. Descriptor minimums are enforced;
   registered modules use them for polling and Fetch now eligibility.
3. Edit generated text, integer, boolean, or choice controls. The demo's optional
   Test secret field exercises private-value handling and makes no API calls.
4. Choose **Save connection**. Configure a saved record again to edit it, or use
   **Delete connection** to remove it and its saved private values.

Secret inputs are always blank when opening saved records. **Saved · leave blank
to keep** means the server holds a value. Leave the field unchanged to retain it,
type a replacement, or tick Clear to remove an optional secret. Required values
cannot be cleared. Previously saved secret values cannot be read back through
the editor/API.

**Discard form** reloads current records and removes local edits. Close/Escape
preserves a dirty form until Save or Discard. A competing save retains your form
and disables Save until you discard it. Connection changes do not affect an
unsaved layout draft underneath the dialog. Ctrl/Cmd+S saves the active connection
form; canvas shortcuts resume after closing it.

## Custom definitions

Copy and edit the example descriptor catalog, then start with:

```sh
cargo run --locked -p paperphase -- serve \
  --connector-definitions /path/to/connector-definitions.json
```

This replaces the bundled catalog. It supplies form definitions only and does
not load an executable plugin. Follow the example's schema version and typed
input structure; see the [settings design](../design/connector-settings.md).
Definitions must continue to describe saved records. An unknown connector,
incompatible changed field, or invalid saved value stops startup with an error;
restore the previous definitions or a backup rather than deleting state blindly.

Use `--connector-bindings` to register modules and explicit source origins for
definitions that should fetch. See [compiled connector operations](compiled-connectors.md).
The plugin package format, UI installation, real provider/account integration,
and the final SDK contract remain pending. These experimental definitions are
not a promise of public plugin compatibility.

## Files and backup

With the default workspace, records are stored in
`.paperphase/workspace.connections.json`, created on first save. A custom
`--state-file` uses the same path with its extension replaced by `connections.json`.
The connection store has its own configuration revision and is protected by the
workspace's process lock. Registered connectors also persist their latest data
and rate-limit metadata in this file, without advancing that configuration revision.

Secrets are stored as plaintext in this owner-readable file, not encrypted.
Stop Paperphase before backing up/restoring it alongside the workspace and any
custom definitions/snapshots. Preserve restrictive file permissions and treat
backups as private. Do not commit these runtime files to Git. On Unix, application
saves create files with no group/other permission bits.

Run `sh scripts/check.sh` for Rust validation and `pnpm --dir web test` after
rebuilding the frontend/executable for browser checks. Setting
`CAPTURE_SCREENSHOTS=1` also updates `docs/design/images/connection-settings.png`.
All current connection tests use demonstration values; real account/provider
integration and Pi validation remain pending.
