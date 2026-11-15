# Changelog

## Unreleased

Current development version: `0.1.0`. The earliest stable stage has not been
reached; minor-version increments per commit begin after that milestone.

Owner accepted the component designer and authorized its commit, merge to
`main`, and GitHub push. This publication also includes the previously local
UIUX redesign commit. Foundation version remains `0.1.0`.

### Enforce screen-wide cleanup for component preferences

- Exposed each component's existing Automatic / Partial when possible / Full
  screen preference directly in Style, with clear screen-wide override wording.
  Evaluate actual changed pixels so an untouched full-preferring component
  between changed neighbours does not force their update to full.
- Added physical-profile count/time maintenance independently of data changes.
  Static screens still receive full cleanup; the shared count resets only on
  successful full acknowledgement. Metadata-only saves and unchanged-pixel
  shortcuts cannot discard overdue cleanup.
- Wake pending workers when health becomes due. Cleanup and recovery override
  soft owner throttling while hardware spacing, retry and quiet hours remain
  enforced. Overdue quiet-hours cleanup resumes with the newest content.
- Kept the qualified three-partial limit and conservative 30-minute ceiling for
  the partial profile. These are Paperphase policy choices, not a prescribed
  vendor 15/30-minute schedule. Device shows effective limits, bounds advanced
  inputs to the profile and explains the shared count/static cleanup.

Verification: `sh scripts/check.sh` passed 192 Rust and 11 Python tests,
including static deadlines, count-only cleanup, mixed preferences, throttled-worker
wakeup, failure recovery and quiet hours. All 39 browser workflows and the ARM64
release build passed. The Impeccable reviewer returned `ship` for the narrow
desktop/mobile control extension; the owner confirmed its wording and placement.
The new build is installed on the Pi with scheduled cleanup enabled, the same
three-partial/30-minute limits, preserved revision 5 and both units active/enabled.
The deployed build completed full startup recovery with no error and revision 5
preserved. Accelerated static-content timing checks remain separate from physical
observations. Version remains `0.1.0`.

### Enable bounded physical partial updates

- Added explicit `--display waveshare-v2-partial` for the confirmed panel,
  retaining the safe full-only profile and simulator default. The shared
  controller enforces 180-second spacing/retry, at most three partials, a 40%
  area limit and a 30-minute full-refresh age when content changes. Startup,
  unknown baseline and failure still recover with a full refresh.
- Added cold/wake partial transfers with row-stride cropping and exact region
  length in place of the pinned vendor method's full-frame padding. Validate
  unchanged outside pixels, previous/target hashes, region, payload length,
  BUSY, sleep and power release before advancing the completed receipt.
- Added an opt-in qualification wrapper preserving saved layouts and private
  completed receipts, with shared hardware ownership, durable cooldown
  reservation, bounded workers and an overall 130-second deadline. Reversal,
  clearing and re-addition run in separate power-releasing processes; observation
  gaps can be extended to 15 seconds without leaving the panel energised.
- Added profile-specific Device guidance and a generic queued-update caption,
  preserving the existing desktop/mobile design. The supplied test-Pi unit now
  selects the explicit partial profile; full-only rollback remains documented.

Desktop verification: `sh scripts/check.sh` passed 185 Rust tests and 11 Python
adapter/qualification tests; all 39 Chromium workflows and ARM64 release build
passed. New contracts cover partial dispatch, exact rectangle and both frame
acknowledgements, retained receipts after mismatches, cold partials without a
hidden full refresh, cropping, isolation, cleanup and private durable writes.
The UX finish reviewer returned `ship` for the narrow profile-guidance extension.

Pi qualification: the first 160×48 partial took 0.418 seconds (960 bytes), with
full baseline 5.746 seconds and complete bounded operation 9.178 seconds. The
owner confirmed correct polarity and unchanged outside content. Two four-partial
cold/sleep/wake sequences completed at approximately 0.412 seconds per transfer
(3.226 seconds including process startup/sleep). The owner confirmed reversal,
clearing, re-addition and unchanged outside content during the slower sequence.
The real app completed a 12-minute cycle: startup full, three automatic partials
(0.491–0.497 s, 664–728×190 regions), then a full refresh for `partial_limit`
(5.744 s), resetting the counter to zero. No errors occurred; the layout revision
stayed unchanged. The owner confirmed the integrated image and clean maintenance
result. Both services remain active and boot-enabled. Longer
lifetime ghosting/power, fast waveforms, physical quiet-hours and interrupted-power
checks remain open. Version remains `0.1.0`.

### Connect automatic Waveshare V2 full updates

- Added an explicit physical backend for the confirmed 800×480 monochrome
  Waveshare 7.5-inch V2 / Rev2.3 HAT. It reuses the compositor, desired/completed
  queue, source/clock scheduling and quiet-hours gates. The default stays the
  simulator; the hardware profile enforces full refreshes at least 180 seconds
  apart with the same retry floor.
- Added a pinned, isolated Python adapter with bounded BUSY waits, watchdog,
  exact-frame acknowledgement, shared hardware ownership and sleep/power
  release. Private writable worker directories support lgpio notifications
  under the read-only service filesystem.
- Persisted conservative attempt deadlines before GPIO access and private
  completed-frame receipts after success. Restart restores a labelled reference
  image, honours cooldown and requires full recovery. Failed updates retain the
  reference; recovery-storage failures have a separate warning. Graceful shutdown
  drains the active bounded operation.
- Distinguished physical pending, completed and unconfirmed images in the editor;
  added full-refresh guidance. Added boot-enabled non-root service examples and
  design/operator notes. Preserved existing Pi workspace, connections and plugins.

Validation: `sh scripts/check.sh` passed 183 Rust tests, four Python adapter
tests, host/Wasm builds and lint checks; 38 Chromium tests passed. ARM64 release
build passed. On the attached Pi, the first integrated full refresh took
5.743 seconds (8.670 seconds including sleep/cleanup); the owner confirmed a
readable image. Source-driven output, two automatic clock updates, retained
restart cooldown and full recovery completed. The temporary clock was removed
and the original layout matched exactly. Both services were active and enabled.
Partial/fast refresh, physical quiet-hours and interrupted-power recovery, and
longer ghosting/maintenance runs remain pending; hardware stage 4 remains open.
The final two physical-state browser tests also passed after adding visible
cooldown/retry captions. The UX reviewer scored that requested fix resolved at
desktop, laptop and mobile sizes. Version remains `0.1.0`.

### Record potential screen improvements after hardware integration

- Added an owner-approved backlog for alternating components, combined feeds,
  pages with pinned information, scheduled views, relevance priority, and
  summary/detail views. Keep physical-display integration next; revisit the
  options after real-panel refresh and ghosting validation.
- Recorded presentation reliability considerations without promising unmeasured
  switching intervals or marking these features implemented.

Validation: planning-only change; local document links and whitespace checked.

### Allow internal overlap and fix design save warnings

- Allowed positioned items to overlap inside their parent component, following
  owner feedback. Kept whole-component separation, content boundaries, minimum
  item dimensions and the reserved data-status area. New items prefer clear
  space but can share the component when no clear rectangle remains.
- Corrected clipping detection: font line spacing alone no longer reports
  clipped text. Reports identify individual clipped items using rendered ink.
  A structurally valid reusable design can be saved or placed with a clear
  warning; genuine clipping remains visible and is not silently discarded.
- Made positioned Fit to contents enlarge clipped boxes without moving items
  or changing fonts, then fit the outer box. Refreshed precision fields after
  fitting so the inspector matches the preview.
- Made icon field selection prefer icon-key/icon-id instead of unrelated text,
  added an explanation for unavailable fields and the icon-name requirement,
  and used friendly field names and one-based item numbering in the designer.
- Isolated automated browser fixtures on a configurable test port so validation
  does not mutate an owner's active workspace. Updated overlap expectations and
  removed fixed-port assumptions from Google callback browser checks.

Validation: `sh scripts/check.sh` passed all 176 Rust tests and host/Wasm checks.
The isolated Chromium fixture passed all 36 tests, including new overlap/clipped
save/fit/icon-field regressions and the existing Google/source/device flows.
Frontend and embedded builds passed. Actual desktop/laptop reproductions were
captured without saved-state writes; the owner's preview workspace was retained.
A fresh source/screenshot reviewer inspected the owner's original image and five
new reproductions, returning SHIP for this desktop follow-up. No new detector
suppression was added. Version remains `0.1.0`; physical Pi/display and real
Google checks remain separate.

### Design reusable compact components before placement

- Added an independent component sandbox for every loaded plugin, existing
  connection and existing component. Start blank or customise a supplied
  template before needing display space. Compact starts use 14 px text, 4 px
  padding and 2 px gaps; Content/Position/Style reuse the approved owner UI.
- Added host-renderer content measurements and bounded compact fitting, with a
  wrapping penalty for awkward strips. Text sizes and all items are preserved;
  precise sizing and explicit free positioning remain available. Current-value
  feedback explains that future longer information may require room.
- Added named plugin-specific design storage alongside the workspace, atomic
  revision-checked save/delete APIs, plugin-relative bindings and source rebinding.
  Library save does not update the layout or completed screen. Failed placement,
  conflicts and failed storage retain the editable design; closing restores the
  previous layout draft and history. Personal design files are ignored by Git.
- Restored Delete for mouse-selected components/internal items with Undo and
  native text-field protection. Added a visible Design this component entry.
  Kept ordinary component resize handles and existing source/Google/device flows.
- Iterated independent UX reviews: repaired route/picker focus, laptop dock
  clipping, saved-design discovery, incomplete-name guidance and excessive
  wrapping. Retained the approved palette, typography, footer and incumbent
  DESIGN.md/token sidecar. Added design/operator documentation and actual local
  desktop/laptop captures.

Validation: `sh scripts/check.sh` passed all 173 Rust tests plus host and Wasm
checks. A clean sequential Chromium run passed all 34 tests (eight designer
workflows plus the existing 26); after the final saved-library refresh and field
label correction, the expanded save/close/rename test and affected weather/RSS
flow both passed again. Frontend type/build and embedded Rust build passed. Two
independent UX assessments rechecked their named fixes; a fresh finish reviewer
returned SHIP for the validated desktop scope after its stale-library and field
label fixes were scored resolved. Five final local captures show actual compiled
UI, with matching-values fit measurements and no browser page errors. Reviewed
functional grid/status-band detector advisories remain documented; no new global
suppressions were added. Pi, real Google account and physical display validation
remain pending. Version remains `0.1.0`; work is on `component-designer`.

### Redesign the owner workspace on UIUX

- Implemented the owner's chosen canvas/task-dock layout: compact navigation,
  large display preview, right inspector and component strip below. Content is
  editable before geometry and styling; conversion to free element positioning
  is explicit. Retained placement, bindings, templates and draft history; the
  owner-reported deletion/internal-designer regressions below remain open.
- Made Data sources a dedicated working page with a service chooser, separate
  demo/testing connectors, grouped plugin fields and forecast options. Google
  accounts have their own view, advanced app setup and prerequisite handling
  before calendar creation. A non-secret same-browser intent resumes calendar
  creation after successful linking.
- Moved quiet hours and advanced refresh policy into Device. Added semantic
  colours, larger readable controls, visible focus, human component names and
  narrower-width reflow. Retained focus through selection, numeric/Tab edits,
  movement and status updates; connection polling updates unchanged lists in place.
- Added recoverable initial loading and optional-metadata failures, plus named
  confirmations for saved connection deletion and account disconnection. Removed
  an unsupported unchanged-frame promise because removal can queue new pixels.
- Added the approved Odd Orbit logo and “Developed by Odd Orbit Software” footer,
  with Orbi on About only. Artwork is served locally from the Rust executable;
  the portrait loads lazily and both PNGs carry their supplied-artwork provenance.
- Recorded the implemented system in `DESIGN.md`, `.impeccable/design.json` and
  `docs/design/owner-workspace.md`; updated operator navigation and saved actual
  local screenshots. Kept the approved plan and earlier alternatives in
  `docs/planning`.
- Triaged the four completion-hook findings and recorded file-scoped detector
  exceptions in `.impeccable/config.json`: the dynamically populated frame image,
  hidden/centered canvas chrome and the owner's explicitly approved paper palette.
  No rules or files were globally disabled; application styling is unchanged.

Validation: frontend TypeScript/build passed; `sh scripts/check.sh` passed all
166 Rust tests and host/Wasm checks. A clean synthetic fixture passed all 26
Chromium tests, including six new UX regression tests. Both confirmation tests
passed again after the final copy correction. Independent finish review scored
its two material fixes resolved: secret-field construction and an element-editor
capture taken during redraw. Fifteen final screens were captured from the
compiled service without UI asset overrides; image loads, page errors, bounds,
documentation links and whitespace were checked. The local synthetic preview
remains available; no Pi, real Google account or physical panel validation was
performed. Version remains `0.1.0`.

Owner acceptance and merge: the owner approved the design for merging to `main`,
with two known issues scheduled alongside the next plugin-specific component
designer: Delete does not reliably remove mouse-highlighted components/elements,
and the internal designer is missing from their interactive workflow. Recorded
visible-entry, mouse-selection/deletion, text-input protection and undo criteria
in `docs/planning/delivery-plan.md`. The passing tests above do not cover these
reported paths; this merge does not claim either issue fixed. The UIUX work is
committed and merged locally; publishing to GitHub is separate.

### Review owner workflows and propose a UI redesign

- Recorded the confirmed everyday-owner audience, desktop priority, warm
  paper/green identity and separate advanced controls in `PRODUCT.md`.
- Completed two independent local reviews covering display editing, data
  sources, Google setup, display settings, keyboard behaviour and recovery.
  Recorded findings and implementation priorities in
  `docs/planning/ui-ux-review-2026-11-15.md`.
- Prepared three workspace directions and four supporting screen/branding
  proposals using synthetic data and approved Odd Orbit/Orbi references.
  Workspace structure and mascot placement await owner feedback.

Validation: inspected the synthetic local fixture in headless Chromium and
source; rendered seven static proposals at 1440 × 900 and checked loaded images,
document bounds and material clipping/state consistency. Review fixture and
reviewer overlay helper exited; the feedback comparison page remains available.
No application source, saved data, real Google setup or Pi state changed.
Runtime suites were not rerun for documentation/proposals. Version remains
`0.1.0`; no commit or push performed.

### Audit source before GitHub publication

- Reviewed Git-includable files and all 12 reachable commits for credentials,
  private runtime artifacts and known token/key patterns. Gitleaks findings were
  minified autocomplete/maxLength code, confirmed false positives rather than
  real secrets. One-way comparisons against three live Google credential values
  found no source/history matches; values were not returned to the session.
- Reviewed new and credential-related screenshots. Actual client/token files,
  local evidence and deployment archives remain outside ordinary Git adds.
  Recorded non-secret local address/path metadata and the audit's scope in
  `docs/usage/source-publication.md`.
- Added ignore rules for downloaded Google client JSON, private runtime files,
  environment files and common key formats, keeping example environment files
  and source fixtures available.

Validation: 12 private-path and five public-source/example ignore checks passed.
Redacted scanner reports and exact-match checks are retained under ignored local
artifacts. Documentation/whitespace checks passed. No runtime source changed;
version remains `0.1.0`. No commit or push was performed.

### First live Google link and setup follow-ups

- The owner enabled Calendar API for the OAuth project and completed a real
  login. A status-only Pi check confirmed one linked account, no linking error
  and no reauthorisation flag; no credential/token or calendar/event values were
  returned to the session.
- Recorded later work for linking UX and import of Google's Web application
  client JSON. Use synthetic fixtures during development and let the owner
  provide the real file directly to the Pi; keep private values out of logs,
  bundles, browser persistence and the public relay.
- Clarified credential-file guidance: Git ignore rules prevent ordinary source
  tracking but do not restrict assistant reads; an outside-checkout location
  reduces accidental inclusion, while access control requires enforced rules.

Validation: owner-confirmed real consent plus redacted Pi status passed.
Documentation links and `git diff --check` passed. No runtime code changed;
current desktop validation remains 166 Rust tests and 20 browser tests.
Multi-account fetches, refresh renewal and Calendar panel rendering remain
pending. Version remains `0.1.0`.

### Explain Google linking failures

- Retain a safe, in-memory failure code/message for known callback attempts and
  display guidance in the return banner and account setup form. Log only the
  fixed code; never reflect provider payloads or OAuth credentials. Clear the
  diagnostic on a new attempt/setup save and prevent replays from replacing it.
- Distinguish Google Calendar API disabled/missing-scope responses from client
  rejection, invalid grants, connectivity and other access failures. The former
  opaque callback did not retain the reported attempt's cause, so a new login
  is needed after this diagnostic update.
- Add provider-error redaction, legacy/modern disabled-API reasons, client
  rejection and diagnostic replay tests; extend browser callback assertions.

Validation: `sh scripts/check.sh` passed 166 Rust tests and host/Wasm checks;
editor build and all 20 Chromium tests passed. ARM64 release build and Pi update
passed, with layout, connections and Google configuration preserved. The new
service started in 50.10 seconds. The saved client was not rejected by a
deliberately invalid code probe, which does not establish Calendar API access.
A fresh owner attempt reported `calendar_api_disabled`, confirming Calendar API
access is disabled for the OAuth project despite the granted read permissions.
The owner must enable that API before consent completion and renewal can be
validated. Version remains `0.1.0`.

### Calendar setup on the Pi — 2026-11-15

- Verified the owner's deployed public callback returns `ok` over HTTPS. Added
  Caddy Docker Proxy label instructions to match the owner's Docker setup;
  no SSH access to the sensitive VPS was used.
- Built the current service for ARM64 and activated it with nine registered
  modules, including Calendar. Kept a private rollback backup, preserved layout
  revision 3 and connection configuration revision 2, and left the existing
  source process and physical panel alone. One observed service startup took
  50.67 seconds; this is not a new resource/performance baseline.
- Checked the Pi-hosted account setup form and Calendar selectors in Chromium.
  Client fields remained blank, the correct public callback was prefilled,
  linking stayed disabled before configuration, and discarding the connection
  form left saved state unchanged. Added a cropped setup screenshot containing
  no credentials.

Validation: ARM64 release build, staged archive checksum, service/API checks,
state preservation checks and Pi browser assertions passed with no page errors.
The previous full desktop check remains 164 Rust and 20 browser tests; this
update changes documentation only in the source checkout. Actual Google client
configuration, account consent, token renewal and Calendar panel rendering
remain pending. Version remains `0.1.0`.

### Google Calendar and hosted browser callback

- Added a read-only Calendar Wasm connector with bounded event windows, recurring
  instances, all-day/timezone handling, privacy filters and agenda templates.
  Derive a combined agenda from enabled calendar connections, preserving source
  labels, duplicate privacy and the oldest input's freshness.
- Added host-owned multi-account linking, private client/token storage, PKCE,
  one-use expiring state, renewal/rotation and reconnect handling. The account
  UI exposes calendar choices and never returns credentials. Calendar settings
  changes invalidate old cached text before another fetch.
- Added a stateless Node/Docker callback for
  `auth.paperphase.oddorbit.software`, with Compose/Caddy examples and owner-run
  deployment instructions. The browser returns to the editor's actual address;
  no fixed Pi hostname, VPS-to-Pi connection or port forwarding is required.
- Documented the protocol, multiple-Pi isolation, Cloud client setup, seven-day
  Testing expiry and bounded-pagination limitations. VPS deployment remains the
  owner's responsibility; no server access was performed.

Validation: `sh scripts/check.sh` passed all 164 Rust tests, formatting, host
and Wasm Clippy/build checks, including the compiled Calendar boundary test.
Editor build and all 20 Chromium tests passed, including cross-site callback
return, blocked cross-site API access and draft retention. Four relay tests,
Docker build, Compose validation and an isolated read-only container HTTP smoke
test passed. Documentation links and `git diff --check` passed. Live Google
consent/renewal, production callback deployment, new Pi resource checks and
physical Calendar rendering remain pending. Version remains `0.1.0`.

### Plan UI updates from GitHub releases

- Require web-UI updates for both the core application and installed plugins
  using GitHub releases, with version/release information and an explicit update
  action. Include this in release/distribution work rather than deferring core
  UI updates until after the initial release; retain SSH for recovery.
- Require a completed physical Updating message before stopping the service,
  retained through the accepted restart, followed by health checking and normal
  content restoration. Record independent activation/recovery ownership,
  compatibility/integrity checks, preserved state, interruption handling and
  rollback acceptance criteria.

Verification: documentation links and `git diff --check` passed. No updater is
implemented by this planning change. Version remains `0.1.0`.

### Plan component design before placement

- Recorded the agreed order: Google Calendar plugin, then a component designer
  that works before placement, then automatic physical-display integration.
- Require a blank or editable-template draft for every loaded plugin, with
  dimensions, bindings and presentation editable even when defaults overflow or
  no display space is available. Keep layout changes isolated until explicit,
  validated placement; retain the design after failed placement.
- Added acceptance criteria for oversized/overflowing defaults, crowded layouts,
  cancellation, plugins without templates and normal editing after insertion.

Verification: documentation links and `git diff --check` passed. This records
planned work only; no editor behaviour changed. Version remains `0.1.0`.

### First physical V2 full refresh

- Added a one-shot bring-up utility for the renderer's 800 × 480 binary PBM,
  using a separately supplied, pinned Waveshare V2 reference. It locks ownership,
  bounds BUSY waits and total operation time, records JSON evidence, requests
  sleep on success and releases HAT power/GPIOs on exit.
- Enabled SPI on the temporary Pi and displayed saved layout revision 3 on the
  attached monochrome V2 panel / Rev2.3 HAT. The full refresh took 5.74 seconds,
  with BUSY asserted then idle; sleep and cleanup completed. The owner visually
  confirmed a correct, readable image. Documented switches, SPI/GPIO mapping,
  reference revision, operating steps and the remaining integration work.
- Corrected the initial test wrapper's missing image data-mode transition;
  complemented bytes now pass through the vendor's D/C and chip-select handling.
  The first attempt timed out and released power. Its diagnostic report is kept
  alongside the successful retry and separate visual-confirmation evidence.

Validation: valid PBM dry run passed; truncated/wrong-resolution inputs were
rejected before GPIO import. A simulated SPI interface checked complete data-mode
planes, valid bytes, chip select, sleep and cleanup. The physical retry passed
and was visually confirmed. `git diff --check` passed; no Rust code changed.
The Rust service still uses its simulator; automatic physical updates, partial
refresh, maintenance and interrupted-update recovery remain pending. Version
remains `0.1.0`.

### Display revision identified

- Recorded the owner's inspected labels: Waveshare HAT revision 2.3 and
  monochrome 7.5-inch screen V2. The vendor documents 800 × 480, matching the
  simulator and replacing the earlier recalled 800 × 400 resolution.
- Selected `epd7in5_V2` as the initial vendor-driver reference while keeping
  current/older V2 selection, GPIO/SPI mapping, physical full/partial refresh
  and maintenance behaviour pending actual validation.

Verification: checked Waveshare's V2 specification and current driver constants;
documentation-only changes pass `git diff --check`. No display was driven.
Version remains `0.1.0`.

### First headless Zero 2 W validation

- Cross-built the current working tree as an ARM64 release application with its
  embedded frontend, then copied it and eight portable Wasm guests to a
  temporary 64-bit Trixie image. No physical display was attached.
- Ran all 31 runtime boundary tests natively, including compiled provider guests,
  infinite-loop interruption, memory limits, HTTP restrictions/timeouts and
  recovery. Passed 30 additional CLI/API assertions covering rendering, saves,
  conflicts, compiled fetching, private settings, quiet hours and restart/cache
  recovery. Six Chromium checks against the Pi's LAN editor covered browser
  editing, saving, reload persistence, completed preview/display equality and
  connector settings without JavaScript errors. Captured an actual screenshot;
  raw reports and artifact hashes remain in ignored local storage.
- Measured 1.94 s temperature-guest compilation and 4.99 ms median invocation
  latency over 100 runs. The eight-module service took 43.39 s to start and
  43.53 s to restart, with 51.2 MiB steady RSS, 52.5 MiB sampled peak RSS and no
  sampled process swap. Recorded this startup limitation and the workload's
  limits in the operator guide; these are initial measurements, not production
  resource budgets.
- Documented the temporary LAN test installation and the kernel-specific Wi-Fi
  workaround used to make the headless image reachable. Updated progress to
  distinguish native Pi evidence from pending Google-account and panel checks.

Validation: `sh scripts/check.sh` passed all 153 desktop Rust tests, formatting,
host/guest Clippy and eight Wasm builds. The frontend build passed. On the Pi,
all 31 native boundary tests and 30 CLI/API smoke assertions passed. Version
remains `0.1.0`; all six LAN browser checks also passed. Long-running workloads
and physical-display validation remain pending.

### Air quality, daily quotes and generic JSON

- Added three compiled plugins: Open-Meteo/CAMS AQI and pollen with hourly
  forecasts; online daily quotes with optional personal/alternating selection;
  and typed JSON Pointer mappings for public or privately authenticated APIs.
- Added editable template boards, units/model attribution, seasonal missing-data
  handling, valid-empty JSON states, strict type checks and optional observation
  timestamps. All reuse host networking, polling, caching and rendering.
- Added bounded row-based settings with text, multiline and choice cells for
  personal quotes and mappings, accessible row labels, isolated defaults and
  authoritative server validation. Optional secret headers remain redacted.
- Added fixed scheduler time and one bounded read of a connection's own previous
  primary dataset. Online quotes reuse persisted content for 24 hours without
  resetting source age; personal selection sends no quotes to the provider.
- Recorded the agreed plugin stretch and the later Leave in X minutes component,
  including calendar/transport dependencies, buffers and scheduled/stale states.
  Added design/usage documentation, provider notices and actual screenshots.

Validation: `sh scripts/check.sh` passed all 153 Rust tests, formatting,
host/guest Clippy and eight Wasm builds. TypeScript/frontend builds and all 18
Chromium flows passed, including row editing and exact preview/display equality.
Mermaid diagrams rendered; new screenshots were reviewed. Isolated live probes
verified Open-Meteo/CAMS and ZenQuotes, plus an unauthenticated local JSON source.
The restarted local service exposes all three plugins and preserves layout
revision 7 and existing connection settings, including private credentials.
Version remains `0.1.0`; Pi and physical-display validation are pending.

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
