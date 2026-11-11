# Changelog

## Unreleased

Current development version: `0.1.0`. The earliest stable stage has not been
reached; minor-version increments per commit begin after that milestone.

### Marked scheduled bus departures

- Show **Scheduled HH:MM** when a bus has no usable live estimate, including
  timetable trips absent from a successful real-time response. Live predictions
  retain priority for the same trip, service date and stop sequence.
- Import service IDs, weekday calendars, date exceptions and boarding types.
  Consider only the next three hours; handle overnight/DST service dates, and
  preserve cancellations, skipped calls and stop reassignments.
- Page selected-stop timetable references into bounded groups, build a per-run
  trip index and memoise metadata lookups. Existing templates display the marker;
  typed rows expose `status=scheduled` and `scheduled-at`, leaving live estimate
  and countdown fields null for timetable entries.
- Added native/compiled regression coverage and updated the canvas screenshot,
  design and usage documentation. The real archive with an empty live feed
  produced **44 · Enniskerry · Scheduled 23:45** for stop 2813.

Validation: `sh scripts/check.sh` passed all 138 Rust tests, formatting,
host/guest Clippy and five Wasm builds. TypeScript/frontend builds and the
Dublin Bus browser flow passed, including exact preview/display equality;
the updated screenshot was reviewed and Mermaid diagrams rendered. A replay
with the real public archive and no live updates verified calendar-based
fallback. The restarted local service preserved layout revision 7 and the key,
and returned **44 · Enniskerry · Scheduled 23:45** with `status=scheduled`, a
separate timetable timestamp and no live estimate/countdown value.
Version remains `0.1.0`; Pi and physical-display validation are pending.

### Dublin Bus and shared reference data

- Added a compiled Dublin Bus connector for up to eight numbered stops per
  connection, with private NTA-key settings, route filters, per-stop/combined
  templates and typed predictions. First requested stop: 2813, Ranelagh Road.
- Use one compact NTA protobuf feed for all selected stops. Decode borrowed
  slices, bound candidates, preserve cancellation/skipped/unavailable states,
  handle valid trips without IDs and avoid invented scheduled/destination data.
  Join signed delays to selected-stop timetables, propagate sparse updates
  forward with NO_DATA/SKIPPED semantics, and handle service dates, overnight
  times and Dublin daylight-saving boundaries.
- Added explicit secret-header grants, bounded larger work profiles and generic
  immutable reference lookups using the existing HTTP pool. Ordinary connector
  budgets remain unchanged; keys are absent from URLs and public settings reads.
- Added enabled-only conditional ZIP/CSV imports, byte-range extraction, CRC and
  size checks, grouped selected-stop streaming, durable last-good tables,
  shared memory admission, automatic
  refresh/retry and preparation status. Generic multiline settings support stop
  lists without removing their text/byte limits.
- Added synthetic fixtures, host/guest/browser coverage, design/operator docs
  and attribution. NTA's token policy requires 60-second spacing; a private-key
  probe returned a 763,765-byte protobuf feed on 2026-11-11.

Validation: `sh scripts/check.sh` passed all 134 Rust tests, formatting,
host/guest Clippy and five Wasm builds. TypeScript/frontend builds and all 15
Chromium flows passed; the bus flow was rechecked after the final guest fix.
Mermaid diagrams rendered and both new screenshots were reviewed. Two
quota-spaced captures exercised real protobuf feeds and streamed public schedules
without committing private keys or live captures. The replay exposed sparse
delay propagation and inconsistent descriptor-route separators; exact trip joins
now resolve route/headsign data while known conflicting routes remain unavailable.
One desktop replay used about 113 million fuel, 10.6 MB guest memory and 37 ms;
the explicit bus grant is 500 million fuel, leaving default budgets unchanged.
The backed-up local service retained layout revision 7, all settings and the key.
Its live 2813 board returned route 44 to Enniskerry, an arrival estimate, explicit
unavailable-time content and matching observation times across all three datasets.
Version remains `0.1.0`; Pi and physical-display validation are pending.

### Luas departures

- Added a Rust/Wasm Luas connector using one shared-host HTTPS request for both
  directions, service messages and provider observation time. Includes the 67-stop
  selector, direction/destination filtering, bounded XML parsing, sorted arrival
  rows, due/unknown/empty states, attribution and two editable board templates.
- Charlemont with both directions is the first requested live connection.
  Defaults are 60-second polling/shared-origin spacing and 120-second staleness;
  no published numeric provider quota was found, so that remains unconfirmed.
- Added optional observation timestamps for additional datasets, persisted by
  the host, so fresh fetches of old transport data remain stale consistently.
  Increased bounded setting choices to 128 and made shared-origin scheduling
  fair when multiple connections request every available slot.
- Added deterministic parser/DST tests, compiled guest and cache integration,
  browser coverage, operator/design documentation and third-party data notice.
  Air quality remains planned after the user's transport-priority change.

Validation: the complete Rust script passed all 117 tests, formatting,
host/guest Clippy and four Wasm builds. TypeScript/frontend builds and all 14
Chromium flows passed; the Luas flow was rechecked while capturing its completed
display frame. Mermaid diagrams rendered and actual screenshots were reviewed.
The local service was backed up/restarted with its saved layout unchanged.
Charlemont's first live connection returned fresh data for both directions,
six retained arrivals and matching observation times in both datasets. Version
remains `0.1.0`; Pi and physical-display validation remain pending.

### Plugin-owned weather icons

- Added static monochrome Meteocons artwork, MIT notice, pinned provenance and
  offline atlas generation under `plugins/weather/assets`. No weather artwork
  is embedded in the main binary.
- Added bounded generic PNG sprite-pack loading, semantic literal/data-bound
  icon elements, nearest-neighbour drawing, clipping, freshness scheduling and
  missing-asset warnings. Registered manifests load at startup.
- Weather now publishes 33 semantic keys across current/hourly/daily datasets,
  using provider day/night flags for current/hourly clear/cloud variants.
  New templates contain icons; existing saved layouts and symbols remain valid.
- Added icon pack/selection/size controls to the internal editor and documented
  custom sprite-sheet replacement, mappings and third-party attribution.

Validation: the complete Rust script passed 108 tests, formatting, host/guest
Clippy and three Wasm builds. TypeScript/frontend builds and all 13 Chromium
flows passed, including icon editing and exact preview/display equality. Mermaid
diagrams rendered and the actual icon editor screenshot was reviewed. The local
service loaded all 33 keys and five sizes after a backed-up restart, preserving
the saved layout revision and connection settings. Existing weather cooldowns
remain in force; cached data gains icon fields on its next successful fetch. Pi
and physical-display checks remain pending. Version stays `0.1.0`.

### Plugin setup diagnostics

- Diagnosed components without data when the service starts without registered
  modules and with a clock pinned by `--now`; settings/templates alone cannot
  execute a plugin.
- Added explicit unloaded-plugin status, disabled Fetch actions with setup
  instructions, template hints, and a fixed-clock warning. Exposed clock mode
  in workspace status and documented the correct live startup command.
- Preserved the existing workspace and connection settings when restarting the
  local service with registered modules and the real clock. Verified successful
  weather and RSS fetches from the user's existing connections.

Validation: the full Rust script passed all 103 tests, formatting, host/guest
Clippy and guest builds. TypeScript/frontend builds and all 13 Chromium flows
passed, including unloaded-plugin/fixed-clock guidance. The running local
service reported both real connections ready and all saved components fresh,
with the same saved layout revision. Version remains `0.1.0`.

### Weather forecasts and RSS plugins

- Added Rust/Wasm Open-Meteo weather with current, hourly and daily datasets,
  local-time forecast labels, metric/imperial display temperatures, condition
  symbols, and hourly rain-window summaries including showers. Air quality is
  agreed as the next separate plugin.
- Added bounded UTF-8 RSS 2.0/Atom parsing, plain-text headlines, namespace/base
  handling, safe links, duplicate filtering, successful empty feeds, and durable
  RSS TTL polling hints. Article pages and images are never requested.
- Added finite decimal/HTTP URL settings, validated connection templates, editable
  component names, per-connection source grants, and shared origin spacing.
  Extended the experimental SDK with atomic multi-dataset publications,
  observation timestamps, and lower-bound polling hints while preserving the
  legacy single-dataset host call.
- Persist all datasets together, mark retained data stale together, preserve
  cooldowns and original-origin hints across configuration changes, and enforce
  aggregate cache/publication budgets. Added operator/design documentation and
  actual UI screenshots. UI package installation remains pending.

Validation: the complete Rust script passed on x86-64 Linux with 103 tests,
formatting, host/guest Clippy and all three guest builds. TypeScript/frontend
builds and all 12 Chromium flows passed. The compiled weather guest handled a
48-hour fixture within the unchanged fuel budget; Mermaid diagrams rendered and
actual UI screenshots were reviewed. A live Open-Meteo fetch through the compiled
guest returned fresh current data, 24 upcoming hourly periods and three daily
forecasts on 2026-11-11. RSS publisher-specific live validation, Pi measurements,
Google accounts, and physical display validation remain separate. Version stays
`0.1.0`.

### Simulated refresh controller

- Added component refresh requests and a saved, validated refresh policy.
  Host-owned synthetic capabilities, alignment, and hard maintenance limits
  decide partial/full mode independently of component requests.
- Compare completed and desired packed pixels, align dirty bounds outward,
  include moved/removed content, skip unchanged frames, and promote updates for
  baseline recovery, count, age, area, or explicit full requests.
- Batch queued frames, serialize active operations, enforce monotonic minimum
  intervals/backoff, and preserve the exact completed revision when newer work
  arrives. Quiet hours prevent new starts while active operations finish.
- Retain completed pixels after failure, retry with a full refresh, and wake
  waiting work when saved intervals change. Added UI mode/reason reporting,
  policy controls, design/operator documentation, and browser coverage.
- Updated delivery order to build initial real-source plugins in the simulator
  before physical hardware. Weather/RSS are the default next candidates.

Validation: the complete Rust check script passed on x86-64 Linux with 89 tests,
formatting, host/guest Clippy, and the compiled guest build. TypeScript and
frontend builds passed; all 11 Chromium flows passed, including policy promotion,
persistence, unchanged manual refresh, and error/recovery status presentation.
Updated Mermaid diagrams rendered successfully and the UI screenshot was reviewed.
Synthetic profile values do not establish physical panel capabilities or safe
maintenance settings. Pi, Google account, and physical display validation remain
pending. Version stays `0.1.0`.

### Internal element editor

- Added optional positioned text/data/clock boxes, with vertical flow retained
  for existing layouts. Rust validates minimum size, bounds, and non-overlap
  inside padding and the reserved host data-status strip.
- Added renderer-measured flow conversion, a cropped component canvas, element
  selection, drag/resize, numeric and keyboard placement, adding/removing
  elements, and flow restoration through the existing history and save paths.
- Preserved explicit font sizes and clipped each element independently. Outer
  resizing and source/padding changes reject arrangements that would cut off
  positioned contents. Added design/operator guidance and an actual screenshot.

Validation: the complete Rust check script passed on x86-64 Linux with 79 tests,
host/guest formatting and Clippy, and the compiled guest build. TypeScript and
frontend builds passed; all nine Chromium flows passed, including measured
conversion pixel equality, collision recovery, internal history, persistence,
hidden-content feedback, and status-strip protection. Pi performance, Google
accounts, and physical display validation remain pending. Version stays `0.1.0`.

### Compiled plugin foundation

- Created the Paperphase Rust workspace, CLI, experimental guest SDK, and a Rust
  temperature example compiled to portable WebAssembly.
- Added Wasmtime/Cranelift preparation with fresh invocation state, shared host
  HTTP networking, explicit origin grants, and strict typed-dataset validation.
- Enforced guest memory, stack, table, fuel, request, response, and publication
  limits. Applied fuel before module startup, disabled redirects/proxies/retries,
  and retained published data only after a successful invocation.
- Added a deterministic local API demo and JSON reports separating runtime
  setup, compilation, invocation, guest memory, and whole-process RSS measurements.
- Added 18 boundary tests, host/guest formatting and lint checks, a check script,
  a locked dependency graph, and GitHub Actions validation.
- Documented the implemented ABI, data structures, execution flow, limits,
  development commands, initial desktop measurements, and outstanding hardware
  and account-linking validation. Added the project README and identity.

Validation recorded: all 18 boundary tests passed in an isolated x86-64 Linux
container, including the compiled Rust guest; formatting, host/guest Clippy,
release builds, and a ten-invocation release demo passed. A Ranelagh Luas HTTPS
forecast response was parsed successfully. Pi measurements, Google OAuth,
provider polling allowances, and physical display validation remain pending.

### Development workflow

- Use short, descriptive commit subjects and keep detailed change descriptions
  in this file.
- Keep the version at `0.1.0` during the current experimental foundation work.
  After the earliest stable stage, increment the minor version on every commit
  and update the workspace package entries in `Cargo.lock` accordingly.

### CI and build outputs

- Include the GitHub Actions workflow that installs the Rust guest target and
  runs the complete host/guest check script on pushes and pull requests.
- Ignore Cargo build output, local caches, compiled native Wasm caches, and
  profiling output so generated files stay out of source commits.

### Shared model and display simulator

- Extracted datasets and validation into `paperphase-core`, independent of
  Wasmtime. Added typed dataset publication to the Rust guest SDK and updated
  the temperature example without changing the experimental host-call ABI.
- Added versioned layouts, stable component/block identities, account-scoped
  data bindings, validation error paths, and bounded geometry/content checks.
- Added snapshot freshness using fetch and optional observation times, source
  errors, stale-content indicators, and distinct missing/empty-data states.
- Added a shared monochrome renderer with a licensed bundled DejaVu font,
  clipped text, wrapping/ellipsis, fixed font sizes, alignment, bounded glyph
  caching, and a packed one-bit framebuffer.
- Added PNG previews and PBM exports from the same pixels, plus change bounds
  covering moved and removed content. Added `paperphase simulate` and sample
  layouts/datasets, implementation diagrams, a generated frame, and usage docs.
- Added model, renderer, and CLI tests, including a compiled-plugin-to-renderer
  workflow. The browser editor, service, scheduling, persistence, and hardware
  validation remain pending; this is still experimental version `0.1.0`.

Validation: `sh scripts/check.sh` passed on x86-64 Linux: formatting, host/guest
Clippy, compiled guest build, and all 40 tests (18 boundary, 7 model, 10 renderer,
5 CLI). Checked every decoded PNG pixel against packed frame data, exercised
input/output alias protection, visually inspected the generated sample frame,
and verified the bundled font checksum and documentation links. Pi and physical
display measurements remain pending.

### Desktop editor and simulated service

- Added `paperphase serve`, embedding a compiled TypeScript frontend in the Rust
  executable, with loopback/LAN binding, host addresses, and simulator status.
- Added separate displayed/draft views, text/data templates, native-pixel drag
  and resize, optional grid/snapping, numeric placement, keyboard controls,
  collision feedback, last-valid-position recovery, undo/redo, and Save/Discard.
- Added a block builder with typed dataset/row bindings, text, number formatting,
  spacing, borders, fixed-size fonts, alignment, and overflow controls. Draft
  previews use the existing Rust renderer; no browser text rasterisation is used.
- Added atomic workspace persistence, exclusive process ownership, revision
  conflicts that retain drafts, and separate saved/completed simulator revisions.
  Kept render work bounded and off async workers; commit work survives a client
  disconnect. Added origin/host checks and request-size limits for the LAN API.
- Added service integration tests and desktop Chromium workflows, embedded-asset
  consistency and browser checks in CI, an actual editor screenshot, and design
  and operator documentation. Version remains experimental `0.1.0`.
- Replaced a bare semicolon in the editor sequence diagram's response label;
  Mermaid treats it as a statement separator rather than message text. Reproduced
  the original error and verified both design sequence diagrams with Mermaid 11.

Validation: frontend type checking/build, `sh scripts/check.sh` (43 Rust tests),
and desktop Chromium workflows passed on x86-64 Linux. Verified preview isolation,
matching preview/completed PNG bytes, failed writes, competing saves, restart
persistence, pointer interactions, bindings, Save/Discard, and retained drafts.
Pi resource measurements, Google account linking, live scheduling, and physical
display validation remain pending. This is the first editor slice, not completion
of delivery stage 2.

### Built-in clock and simulated refresh scheduling

- Added host-rendered time/date sources with IANA timezones, 12/24-hour time,
  English long/short/ISO/weekday dates, and a two-block Time & date template.
  Added per-block clock controls using the host's timezone catalog.
- Added minute/local-date and bound-snapshot freshness deadlines, skipped missed
  ticks, backward-clock handling, unchanged-frame suppression, and a frame ID
  separate from layout revisions so browsers receive clock-only pixel changes.
- Added optional timezone-aware quiet hours to atomic workspace saves, including
  settings Undo/Redo and Discard. Paused updates retain completed pixels and
  render the newest saved content when quiet hours end. Older workspaces load
  with quiet hours disabled.
- Added Refresh display through the same gate, pending/error/paused status, and
  completion checks that prevent old or newly quiet/outdated frames from being
  published. Added clock/refresh design notes, diagrams, usage instructions, and
  actual screenshots. Direct positioning inside components remains later work.

Validation: `sh scripts/check.sh` passed with 54 Rust tests, including calendar/
DST boundaries, missed ticks, backward adjustments, quiet pause/resume, competing
completions, unchanged pixels, old workspace compatibility, and settings
persistence. Frontend type checking/build and all four desktop Chromium workflows
passed. Both updated Mermaid diagrams rendered to SVG in Chromium. The compiled
temperature guest is 89,886 bytes (previously 89,882). Version remains `0.1.0`; Pi measurements,
provider scheduling, Google linking, and physical refresh validation remain pending.

### Connector settings and connection records

- Added bounded, typed connector definitions for text, integers, booleans,
  choices, and write-only secrets, with defaults, required values, UTF-8 limits,
  polling bounds, and field-specific errors that do not quote submitted values.
- Added generated Connections forms with independent Save/Discard/Delete,
  retained forms after conflicts, secret presence/replace/clear controls, and
  keyboard handling that preserves an underlying layout draft.
- Added a separate revisioned connection store with atomic persistence,
  owner-only Unix file permissions, restart validation, and no secret values
  returned by the API. Omitted secrets retain their saved value; explicit null
  clears an optional secret. Secrets are plaintext in the private runtime file.
- Added a labelled settings-only demonstration catalog and optional
  `--connector-definitions` input. This slice does not execute connectors, fetch
  provider data, or install plugins. Added design/usage notes and an actual UI
  screenshot; the public plugin/SDK contract remains experimental.

Validation: `sh scripts/check.sh` passed with 61 Rust tests, including definition/
value bounds, secret handling, old-workspace coexistence, restart persistence,
custom descriptors, competing saves, failed writes, and layout independence.
Frontend type checking/build and all six Chromium workflows passed, including
generated fields, secret placeholders, discard/clear, independent conflicts, and
retained layout drafts. Version remains `0.1.0`; compiled connector integration,
provider scheduling, Pi measurements, Google linking, and physical display
validation remain pending.

### Compiled connector fetching and polling

- Added local portable-Wasm registrations with explicit source-origin grants,
  one shared runtime/HTTP client, one active guest, and one bounded HTTP attempt
  per service invocation. Added a bounded `settings_json` host call and SDK
  settings reader; extended the Rust example with location/units and empty data.
- Connected saved records to automatic polling and Fetch now with persisted
  attempt deadlines, shared-origin floors, exponential backoff, and HTTP
  Retry-After hints that survive invocation failure and normal service restart.
- Persisted latest account-scoped datasets and retained stale last-good content
  after failures. Kept successful empty results distinct. Added cache/file bounds,
  generation checks for late results, and storage recovery that prevents new
  requests until pending retry metadata is durable.
- Added a data epoch separate from layout/config revisions, invalidated obsolete
  frames, refreshed browser dataset choices, and preserved drafts during data
  updates. Fetching continues during quiet hours while completed pixels remain
  held. Added per-connection status and eligibility controls.
- Added the Rust `demo-source` command, an example registration, compiled-source
  browser coverage, implementation/usage documentation, and an actual screenshot.

Validation: `sh scripts/check.sh` passed with 74 Rust tests, including the compiled
Rust guest through service polling, quiet hours, empty/error cases, durable caches,
restart cooldowns, settings edits/recreation, cache limits, and obsolete frames.
Frontend type checking/build and all seven Chromium workflows passed, including
Fetch now → dataset binding → Save → matching preview/completed PNGs. Mermaid
diagrams rendered in Chromium; the Rust demo source returned its documented JSON
and stopped cleanly. The updated example Wasm is 145,407 bytes. Version remains
`0.1.0`; real provider/account rules, UI installation, final plugin contracts,
historical storage, Pi budgets, and physical refresh validation remain pending.
