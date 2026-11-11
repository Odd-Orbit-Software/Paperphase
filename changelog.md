# Changelog

## Unreleased

Current development version: `0.1.0`. The earliest stable stage has not been
reached; minor-version increments per commit begin after that milestone.

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
