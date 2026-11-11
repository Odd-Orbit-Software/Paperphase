# Show Luas departures

Build the plugins with `sh scripts/check.sh` and start the service with:

```sh
target/debug/paperphase serve --connector-bindings plugins/bindings.example.json
```

The example registration includes weather, RSS and Luas, so build all registered
modules before starting. Omit `--now` for live operation. No Luas API key is needed
for the inspected public feed.

1. Open **Connections**, add a connection and choose **Luas departures**.
2. Select **Green · Charlemont** and **Both directions**. Both are defaults.
   Keep three trams per direction initially and leave the destination filter
   blank. Give the connection a recognisable name, such as **Charlemont trams**.
3. Save the enabled connection. The host fetches when eligible; check its status
   for a completed fetch or a retained-data error.
4. Close Connections, enter **Edit layout**, and add **Luas board** from that
   connection. Make a free 464 × 320 area first if needed. The narrower template
   needs 320 × 288. Save to display.

The main board shows separate inbound/outbound sections, destinations, minutes,
the feed's service message, last update and attribution. Direction labels come
from the provider; use destinations to confirm your journey. **Edit contents**
lets you move/resize either direction or bind individual `arrivals` rows to your
own text elements. The aggregate fields are in `stop-board`.

Choose one direction or enter a case-insensitive destination substring to limit
results. The main two-column template still shows both headings when only one
direction is selected; remove/rearrange the unused column in the editor if wanted.
Increasing the number of trams may require larger text boxes. Font sizes remain
explicit, and render warnings identify content that does not fit.

## Timing and empty results

Times are estimates **as of the last successful forecast**, not a local countdown.
The default/minimum fetch interval is 60 seconds, with a shared 60-second spacing
across this provider. More stop connections therefore update less often. Fetch
now respects existing deadlines. Refresh display only renders cached data.

The example marks data stale after 120 seconds, using the feed's observation time
as well as fetch time. A provider failure keeps the last good estimates with a
stale indicator. Never treat retained `Due` text as a newly confirmed arrival.
A successful empty response shows **No trams forecast** and keeps the service
message; an empty filter result shows **No matching trams forecast**. Unknown
arrival times show **Time unavailable** instead of zero.

The 60-second setting is a conservative Paperphase default; the official listing
did not publish a numeric quota. Check current provider guidance before changing
the deployment's aggregate request rate. The feed has no timezone field; the
plugin interprets its timestamp in Europe/Dublin, including daylight saving.

## Attribution and stop-list maintenance

Templates include **Luas / TII · CC BY 4.0 · formatted**. Keep source/licence
attribution visible in customised displays and retain the plugin's `NOTICE.md`
when sharing it. Forecasts are reformatted, sorted, filtered and possibly limited;
Paperphase is not an official Luas application.

The bundled 67-stop selector was retrieved on 2026-11-11. It is not fetched on
every poll. Updating it currently requires updating `plugins/luas/stops.json` and
its matching choice list in `examples/simulator/connectors.json`, rebuilding the
plugin/host, then restarting. UI package installation remains pending.

See [design, data fields and provider references](../design/luas.md).
