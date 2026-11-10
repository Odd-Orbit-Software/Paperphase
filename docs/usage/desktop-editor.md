# Run the desktop editor

From the repository root, build and start the Rust service:

```sh
cargo build --locked -p paperphase
cargo run --locked -p paperphase -- serve
```

Open <http://127.0.0.1:8080> in a desktop browser. The executable contains the
compiled frontend assets; Node is not needed to run it. The default workspace is
`.paperphase/workspace.json`, created on first launch. The default layout leaves
space below the header/weather/agenda components for new components. Sample data
is fictional and makes no API requests. The host clock drives time/date blocks
and marks old fixtures stale. Add `--now 1700000000` for deterministic fixture
freshness; this also freezes time/date blocks until the service restarts.

For access from another LAN computer:

```sh
cargo run --locked -p paperphase -- serve --listen 0.0.0.0:8080
```

Open `http://<host-IP>:8080`. There is no login. Known host IPs, the machine's
hostname/`.local` hostname, and localhost are accepted; reverse-proxy/custom
hostname configuration is not part of this slice. Binding LAN exposes the editor
to other users who can reach that port.

## Edit and save

1. **On display** shows the last completed simulator frame. **Edit layout** opens
   a separate draft; select components on the canvas or in the left panel.
2. Drag to move, or use the bottom-right handle to resize. Red means the proposed
   rectangle is invalid. Dropping it restores the last valid position. Snap uses
   an eight-pixel grid; numeric placement and arrow keys allow exact adjustments.
3. Change content in Properties. Blocks flow vertically; fonts retain their size
   as the component changes shape. Data blocks select a dataset, field, and row.
   Rendering warnings and stale-data information appear below the blocks.
4. Use Undo/Redo or switch to On display to compare. Save writes the layout and
   starts a simulated refresh; Discard reloads the last saved layout. A tab with
   an outdated base revision retains its draft and asks you to discard before
   editing the newer layout.

Keyboard: arrows move one pixel, Shift+arrows eight pixels, Delete removes a
selected component, Ctrl/Cmd+Z undoes, Ctrl/Cmd+Shift+Z redoes, Ctrl/Cmd+S saves,
and Escape cancels a drag or clears selection. Text inputs use native text undo.

## Time, date, and quiet hours

In Edit layout, add **Time & date**. It starts with separate time/date blocks in
Europe/Dublin, using 24-hour time. Each block has its own timezone; change both
when moving the entire clock to another timezone. Choose time, date, or both,
12/24-hour time, and long/short/ISO/weekday date formats in Properties. Typography
and placement work like other blocks. With a real clock, time updates at minute
boundaries and date-only blocks at local date boundaries. `--now` freezes both.

**Display settings** opens quiet hours. Enable it, choose a timezone and pause/
resume times, then Save. The interval may cross midnight. Equal start/end times
are rejected; disable the checkbox to allow updates throughout the day. Undo,
Redo, and Discard include these settings.

During quiet hours, the header says **Quiet hours · paused** and On display keeps
the last completed pixels. Saved edits remain pending. At the end of quiet hours,
the newest saved layout is rendered with the current clock and freshness. Drafts
can still be edited and previewed while paused.

**Refresh display** evaluates the saved layout immediately. It respects quiet
hours and skips a refresh when pixels are unchanged; it does not fetch new API
data or render an unsaved draft. The simulator initialises its frame anew on
startup, so it does not preserve pre-restart pixels during a quiet period.

See the [clock and refresh design](../design/clock-and-refresh.md) for state and
boundary behaviour. Panel-specific refresh budgets and maintenance are pending.

## Alternate layouts, fixtures, and backup

```sh
cargo run --locked -p paperphase -- serve \
  --state-file /tmp/paperphase-demo/workspace.json \
  --seed-layout examples/simulator/layout.json \
  --data examples/simulator/data.json \
  --now 1700000000
```

`--seed-layout` applies only when the workspace does not exist. Existing saved
layouts always take precedence. Stop the service before copying/restoring the
workspace; copy custom snapshot files separately. Keep a backup before editing
JSON manually. Invalid state stops startup with an error; it is never silently
replaced. A `.lock` file alongside the workspace can remain after exit; the OS
releases its lock automatically. Only one service may own a workspace at a time.

To change the display profile, create a new workspace with the desired seed
layout. Changing panel identity/dimensions through this editor's save API is
rejected. No Pi hardware configuration is required for the simulator.

## Frontend development and verification

Use Node 24 or newer and pnpm 11.5.1. Build frontend assets **before** rebuilding
the Rust executable, because Rust embeds `web/dist` at compile time:

```sh
pnpm --dir web install --frozen-lockfile
pnpm --dir web build
cargo build --locked -p paperphase
sh scripts/check.sh
pnpm --dir web exec playwright install chromium
pnpm --dir web test
```

`web/src` is the editable TypeScript/CSS/HTML source. Commit the generated
`web/dist` files with source changes so Rust-only builds work without Node. CI
rebuilds them and checks for differences before running Rust and browser tests.
The browser suite starts/stops its own fixture service on `127.0.0.1:8097` and
uses a temporary workspace. Set `PAPERPHASE_TEST_BINARY` to an absolute executable
path when using a different Cargo output directory. For screenshot updates:

```sh
CAPTURE_SCREENSHOTS=1 pnpm --dir web test
```

This updates `docs/design/images/editor-overview.png` and `clock-quiet-hours.png`
from the tested UI. Playwright
failure traces and local workspaces are ignored by Git. Rust checks need the
compiled guest target and loopback access as described in
[foundation-harness.md](foundation-harness.md). These are desktop checks; they do
not establish Pi memory/performance, Google OAuth, or physical panel support.
