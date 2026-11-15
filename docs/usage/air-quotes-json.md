# Set up air quality, daily quotes and JSON data

Build with `sh scripts/check.sh`, then run the real-clock service:

```sh
target/debug/paperphase serve --connector-bindings plugins/bindings.example.json
```

The example now registers seven plugins; the validation script builds all of
their Wasm modules. Omit `--now` for live use. Package installation through the
UI remains separate planned work.

## Air quality and pollen

In **Connections → New connection → Air quality and pollen**, enter the location
label and coordinates. Dublin is the starting default. Choose forecast hours
and a pollen focus, then save. Add **Air quality now**, **Air quality outlook**
or **Pollen board** from the layout editor.

AQI is the European scale. Concentrations are µg/m³; pollen counts are grains/m³.
The display identifies model estimates. Unavailable pollen is different from
zero and can depend on region/season. Partial forecast peaks are labelled.
Keep the CAMS/Open-Meteo attribution when editing templates. Free API access is
for non-commercial use under the [provider terms](https://open-meteo.com/en/terms).
Historical graphs are not part of this slice.

## Daily quote

Choose **Daily quote** and **Online daily quote** for ZenQuotes. No account/key
is needed for this endpoint. Its default polling interval is 86,400 seconds;
manual actions still respect host/provider spacing. The online quote is cached
for 24 hours even when personal/mixed polling is more frequent. Keep the displayed
`https://zenquotes.io/` attribution. API failures retain the last good quote
with the normal error/stale state.

Optionally use **Add row to Personal quotes** to enter quote/author pairs, then
select **My quotes** or **Alternate online and my quotes**. Delete unwanted rows
with their Remove action. Personal-only mode needs at least one entry; the list
is never sent to ZenQuotes. Selection uses the configured IANA timezone and list
order. It is stable for each date and changes on the next poll, not a continuous
clock tick. For personal/mixed mode, set a shorter polling interval (for example
300 seconds) if you want selection to follow midnight more closely.

Add the **Daily quote** component. Long quotes may need a larger text box or
smaller font through **Edit contents**; the usual overflow reports apply.

## Generic JSON

Choose **Generic JSON**, give it a label and enter an HTTP(S) endpoint. Leave
private header fields blank for a public endpoint. For authenticated endpoints,
enter the complete Authorization value (such as `Bearer …`) and/or X-API-Key.
Saved secrets are not prefilled when reopening the form; blank keeps them.

Set **Records pointer** to the array/object you want, or leave it blank for the
whole document. Add mapping rows with a field ID, pointer, explicit type and
optional unit. Pointers are relative to each selected record. For example:

```json
{"observed_at":1700000000,"sensors":[{"name":"Office","temperature":20.5,"open":false}]}
```

| Setting | Value |
| --- | --- |
| Records pointer | `/sensors` |
| Observation timestamp pointer | `/observed_at` |
| Field `room` | `/name`, Text |
| Field `temperature` | `/temperature`, Number, unit `C` |
| Field `open` | `/open`, Boolean |

The optional timestamp pointer is read from the original document and expects
Unix seconds. Blank uses fetch time. A configured but absent/invalid timestamp
fails the fetch. Missing mapped leaves are unavailable; numeric strings are not
silently converted to numbers. An empty array is a successful **No records**
result. Errors retain the previous data.

Add **JSON summary**, or bind custom elements to the `records` dataset. Reduce
record/field count or enlarge the component if the summary exceeds its space.
The source is limited to 64 KiB; the summary is limited to 4,096 bytes. Default
staleness is 15 minutes; adjust registration's `stale_after_secs` when your source
has a deliberately longer publication interval.

Design, data structures and screenshots are in [the design document](../design/air-quotes-json.md).
The later **Leave in X minutes** component is recorded in
[the implementation plan](../planning/next-plugins.md#later-leave-in-x-minutes).
