# Dublin Bus fixtures

Synthetic GTFS-Realtime protobuf snapshots: `trip-updates.pb` has two stops,
absolute predictions, a skipped call, an unavailable time and a cancelled trip.
`empty.pb` has a valid full-dataset header with no entities. Both use epoch
1700000000 and `fixture-v1`. `delay-updates.pb` supplies signed arrival/departure
delays with an explicit service date and stop sequence. `sparse-updates.pb`,
`no-data-updates.pb` and `skipped-updates.pb` exercise forward propagation and
its stop/skip rules. IDs and forecasts are illustrative.

`reference.json` is the matching lookup data used by compiled/native tests.
`reference.zip` contains synthetic stops/routes/trips/feed-info/stop-times/calendar CSVs and a shapes
member that the importer must ignore. It also contains a non-DB stop and a DB
stop from another county prefix to exercise filtering and identifier handling.

Fake keys in tests are literal fixture strings. User keys and live captures are
never part of these fixtures. Formats follow the references in `../NOTICE.md`.

Timetable rows are paged by stop and include boarding type. Service calendars
and date exceptions support marked scheduled fallback. Cancelled/skipped fixture
trips verify that the fallback does not override those states.
