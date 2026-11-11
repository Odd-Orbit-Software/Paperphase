# Run a compiled connector

The service can execute local portable Wasm modules using saved connection
settings. This is an experimental registration path; UI package installation and
real weather/calendar/Luas integrations are separate work.

Build the host and Rust example from the repository root:

```sh
rustup target add wasm32-unknown-unknown
cargo build --locked --release -p paperphase-temperature-example --target wasm32-unknown-unknown
cargo build --locked -p paperphase
```

Start a deterministic Rust source in one terminal:

```sh
target/debug/paperphase demo-source
```

It serves `{"temperature_c":18.5}` at `http://127.0.0.1:9001/observations`.
This is demonstration data, not live weather. Stop it with Ctrl+C.

In another terminal, start Paperphase:

```sh
target/debug/paperphase serve \
  --connector-bindings examples/simulator/temperature-bindings.example.json
```

Open <http://127.0.0.1:8080>, then:

1. Open Connections and add a **Temperature connector** record. Set its name,
   location label, units, and polling interval, then Save.
2. The enabled connection is fetched automatically when due. Fetch now requests
   an earlier fetch when eligible; its disabled button shows the next eligibility
   time in the tooltip. The example's minimum is 30 seconds across its origin.
3. Return to Edit layout, add a Data field, and select the new connection's
   `current-weather` dataset. Bind `display-temperature` for the selected units
   or `temperature` for raw Celsius. Save the layout to display it.

The source and host can be restarted independently. A stopped/malformed source
retains last good data with a stale indicator and backoff. A null temperature is
an empty successful result. Fetching continues during quiet hours; the displayed
frame updates with the newest data when the pause ends. Refresh display does not
fetch or bypass the pause. Avoid `--now` for normal operation: it freezes the
scheduler's clock as well as time/date blocks.

## Register other modules

The example registration file supplies a connector-definition ID, Wasm path,
source URL, and stale-after seconds. Relative module paths resolve from the
registration file's directory, not the shell working directory. Use an absolute
module path for a custom Cargo target directory. Definitions come from the
built-in catalog or `--connector-definitions`.

The source URL grants its exact origin. The module receives only its connection's
settings and that grant. Service invocations permit one HTTP request, with the
host's timeout, response, memory, and fuel budgets. Only portable Wasm is accepted;
native compiled caches are not accepted. Registration is read/compiled at startup.
Restart the service after changing it. A valid compiled module still needs the
experimental imports/exports and a valid published dataset to fetch successfully.

Registered code can read its own secret settings and publish data. Choose modules
accordingly; sandboxing limits access/resources and does not make arbitrary code
safe to give credentials. Settings-only descriptors have no Fetch now action until
their ID is linked to a module.

Polling intervals, failure backoff, and HTTP Retry-After apply to automatic and
manual fetching. A cooldown is shared by all registered connections using an
origin. Editing settings or recreating a connection does not bypass it. Keep the
complete private connection file across updates/restarts; do not remove its rate
metadata to force a request. Validate real provider polling/attribution rules
before deploying an integration.

## State and tests

The private `workspace.connections.json` file now includes latest snapshots and
origin deadlines alongside settings. It retains its own configuration revision;
background fetches do not conflict a layout/settings form. Back up and restore the
entire file as described in [connections.md](connections.md). Storage faults stop
new fetching until pending metadata is durable again; provider-specific recovery
after a power loss remains pending.

Run `sh scripts/check.sh` for the complete host/guest check. Browser checks now
also require that compiled guest artifact:

```sh
pnpm --dir web build
cargo build --locked -p paperphase
pnpm --dir web test
```

With custom output directories, set `PAPERPHASE_TEST_BINARY` and
`PAPERPHASE_TEST_PLUGIN`, `PAPERPHASE_TEST_WEATHER_PLUGIN`, and
`PAPERPHASE_TEST_RSS_PLUGIN` to absolute artifact paths. The browser harness starts a
private local source and registration, then stops them and removes their temporary
files. `CAPTURE_SCREENSHOTS=1` updates the compiled-connector screenshot too.
No real account credentials or Pi hardware are used by these checks.

See [weather and RSS](weather-and-rss.md) for the first provider plugins, dynamic
feed URL grants, multi-dataset results, and connection templates.
