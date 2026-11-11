# Dublin Bus connector

Status: implemented; desktop and live validation are recorded in the
[design](../design/dublin-bus.md) and changelog. The requested first stop is **2813**, with
multiple stop numbers accepted per connection and the API key entered privately
in the generated settings panel. This is separate from the working Luas plugin.

## Verified provider constraints

- Use the current NTA GTFS-Realtime service. The previous stop-oriented RTPI
  service was retired; do not build against that interface.
- The [NTA portal](https://developer.nationaltransport.ie/) requires a developer
  subscription/key. Its [usage policy](https://developer.nationaltransport.ie/usagepolicy)
  allows one request per token every 60 seconds. Do not alternate keys to
  increase the request rate. Multiple configured stops share one feed request.
- The [published schedule catalog](https://www.transportforireland.ie/transitData/PT_Data.html)
  provides CC BY 4.0 data. Its realtime-operator ZIP was 106,087,599 bytes on
  2026-11-11. Inspecting its directory and stop table with bounded byte ranges
  avoided downloading shapes and stop-time tables.
- The current stop table maps public code `2813` to ID `8220DB002813`, named
  **Ranelagh Road**. Resolve public stop codes from metadata rather than assuming
  an ID prefix for every operator or stop.
- The NTA [upgrade notice](https://www.nationaltransport.ie/ga/news/attention-developers-upgrade-to-gtfs-realtime-api/)
  says matching current real-time data to older static schedules is not
  guaranteed. Stop/route/trip metadata therefore needs a refresh strategy.
- An authenticated probe confirmed `x-api-key`, the v2 TripUpdates endpoint and
  protobuf responses. Multiple stops are filtered locally from one national feed.
  Most Dublin Bus calls supply signed delays rather than absolute times, requiring
  an exact timetable join before they can be displayed as arrival predictions.

## Implementation boundaries

Keep stop lists, route filtering and display templates in the connector. Keep
HTTP, secret handling, request spacing, cache budgets and frame rendering in
shared host services. The key must be an explicitly granted request header for
its provider origin, never a URL parameter or a browser-readable saved value.

The existing 64 KiB guest HTTP buffer suits weather and stop forecasts. Do not
silently remove that bound for all plugins to accommodate a national feed.
The measured feed and schedule joins use a trusted per-registration 8 MiB
response/500 million fuel grant. Other connectors keep their existing defaults. Stream the large static
stop-times member and retain selected stops only; do not load its entire CSV.

Implemented datasets are one summary row per selected stop and typed departure rows
with stop number, route, destination, predicted time, provider observation time
and prediction status. Preserve the distinction between real-time, scheduled,
cancelled, unavailable, valid empty and stale content. A missing trip/static join
must not invent a destination or turn an unavailable forecast into a due bus.
The requested scheduled fallback uses an explicit Scheduled label and service
calendar checks; cancellations/skips retain priority over timetable entries.

The settings panel and compiled module use the standard connection/editor flow.
Reference preparation is visible and does not consume a real-time request slot.
Keys remain private in saved settings and are never committed as test fixtures.

## Completion checks (desktop completed; Pi pending)

- Validate stop lists and route filters, including multiple stops, unknown codes,
  duplicates, leading zeros and invalid input.
- Exercise actual compiled code with realistic bounded provider fixtures.
- Verify exact-origin credential headers, redaction, quota persistence and one
  request covering multiple stops.
- Verify observation-time staleness, cancellation/empty/error behaviour, usable
  templates and preview/display equality in the editor.
- Check the first live stop after the user saves their key. Keep Pi memory/CPU
  measurements distinct from desktop validation.
