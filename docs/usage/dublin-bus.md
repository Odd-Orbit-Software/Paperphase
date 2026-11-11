# Show Dublin Bus departures

Run `sh scripts/check.sh` to build/validate the registered modules, then start:

```sh
target/debug/paperphase serve --connector-bindings plugins/bindings.example.json
```

Use the real clock; omit `--now`. The example registers weather, RSS, Luas and
Dublin Bus, so all corresponding Wasm files must exist. Generated reference data
is stored in `.paperphase/reference/nta.json` by the example configuration.

1. Create an account at the [NTA developer portal](https://developer.nationaltransport.ie/),
   subscribe to GTFS-Realtime and obtain your key.
2. In **Connections**, choose **New connection → Dublin Bus departures**.
3. Enter one or more **Bus stop numbers**, separated by commas or lines. The
   requested first stop is **2813**, Ranelagh Road. Up to eight unique stops are
   supported; repeated numbers and leading zeros are normalised.
4. Paste the key into **NTA API key**, leave the interval at 60 seconds initially,
   choose a maximum of one to six buses per stop and optionally filter routes.
   Save the enabled connection. Future edits can leave the key blank to retain it.
5. Allow the first reference import to finish (including streamed timetable data). **Preparing reference data** is
   distinct from fetching the live feed. Subsequent starts reuse a valid cache.
6. Close Connections and enter **Edit layout**. Add **All bus stops** for one
   combined board, or **Bus stop board** for the first stop. Make room, then Save.

The combined board needs a 600 × 384 area; the single-stop template needs
464 × 304. More stops/long names may need larger element boxes or fewer displayed
buses. **Edit contents** exposes normal position, size, font and binding controls.
For custom departure-row designs, bind `status` to show the `scheduled` marker
and `scheduled-at` for the timetable timestamp. `expected-at` and `due-minutes`
are empty for scheduled rows so they cannot silently appear as live countdowns.

To build separate stop cards, bind their fields to the appropriate `stops` row
(zero is the first configured stop). Individual prediction rows are in `arrivals`.

## What the board means

Times are forecasts as of the displayed update, not a local countdown. **Due**
means the retained estimate has reached that update time. **Cancelled**, **Does
not stop** and **Time unavailable** have distinct meanings. Missing or stale data
must not be read as a newly confirmed bus arrival.

**No upcoming departures** is a successful empty result, not a statement that no
scheduled services exist. The connector uses absolute predictions or joins reported delays to the exact
trip/stop/sequence timetable and service date. When an estimate is missing, it
shows **Scheduled 23:45**, for example, with an absolute Dublin-local time. This
is a timetable entry, not a live forecast or confirmation that the bus will run.
Trips absent from a successful live response can also use this fallback after
checking service dates, weekdays and holiday exceptions. Only the next three
hours are considered; cancelled, skipped and non-boarding calls do not become
scheduled departures. Sequence-only updates resolve through matching selected trip schedules. Sparse delays carry forward to later selected stops, stopping at an unavailable
update; skipped stops preserve the prior delay. Cancellations use matching
selected timetable calls when possible. Unidentified trips cannot always be
associated with your stop. Unknown static trip details are shown as unavailable, never guessed.

One connection's stops share one NTA request per minute. NTA's policy limits each
token to one request every 60 seconds; don't use alternating primary/secondary
keys to increase polling. Other applications/Pis using the same token also count
towards that provider limit. Paperphase's local scheduler cannot coordinate them.
Fetch now respects the same deadlines; Refresh display only renders cached data.
Quiet hours pause display updates while fetching continues.

## Reference data and recovery

Enabled connections check the static archive once an hour with HTTP validators.
Changed metadata is imported using ranges for the required CSV tables. Shapes
are skipped; stop times are streamed and only selected stops are retained. The
first import or changing selected stops reads roughly 42 MB of compressed public
data with the inspected archive, so allow preparation to finish. An unchanged
hourly check reuses the cache. Adding stops prepares them before a real-time fetch. Last good references stay available on failure.
The static metadata request carries no real-time API key.

The local cache is regenerable and contains public reference data. Back up the
private connection/workspace files to preserve settings, secrets and cooldowns;
include the reference cache if you want a warm restart without reimporting it.
Don't delete connection rate metadata to force a request.

Registration paths resolve relative to the bindings file. For a system install,
point `reference_archive.cache` at a writable state directory, for example
`/var/lib/paperphase/reference/nta.json`. The public archive must honour byte
ranges and supply the expected CSV columns. Import errors retain prior data and
are logged without your API key. Check connectivity, disk permissions and the
reference-import log if preparation does not complete.

A 401 fetch error usually needs a valid subscribed key; correct it in Connections.
A malformed/unsupported response or exceeded budget retains last good content.
Unknown stop numbers fail before making the real-time request. Double-check your
number and allow the reference import to complete before retrying.

Keep the template's NTA attribution/licence/as-is statement when customising it.
The [data notice](../../plugins/dublin-bus/NOTICE.md) and
[design](../design/dublin-bus.md) explain source, limits and validation. Pi memory,
CPU and physical display measurements remain pending.
