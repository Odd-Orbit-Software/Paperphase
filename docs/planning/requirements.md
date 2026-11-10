# Requirements

Status: agreed requirements from the planning conversation; unresolved details are
identified explicitly. Foundation implementation has started; see the
[implemented harness](../design/plugin-foundation.md).

## Purpose

Run an efficient, extensible information display on a Raspberry Pi Zero 2 W.
Users install compiled plugins, connect data sources, and arrange their components
in a browser editor representing the physical display.

## Project identity

The project is named **Paperphase**, developed under **Odd Orbit Software**.
The repository is [Odd-Orbit-Software/Paperphase](https://github.com/Odd-Orbit-Software/Paperphase).
The organisation domains are
[oddorbit.software](https://oddorbit.software) and
[oddorbitsoftware.com](https://oddorbitsoftware.com). Use Odd Orbit Software as
the project attribution identity.

## Platform and deployment

- Rust for the core service.
- Sandboxed WebAssembly plugins compiled to native machine code for execution.
  Plugin authors can share packages through GitHub or similar sites. Final
  runtime selection depends on Zero 2 W measurements; the package format and
  versioned host API still need specification.
- One independent service on one Pi with one attached display initially.
- Headless, 64-bit Raspberry Pi OS Lite, mains power, automatic startup at boot.
- LAN access without an application login.
- Shared host facilities for HTTP, scheduling, configuration, and other common
  dependencies. Plugins must not each supply a separate networking stack.
- Plugin installation through the web UI from a package upload or GitHub release
  URL. Restarting after installation is acceptable.
- TypeScript compiled to browser JavaScript for the frontend, served by the Rust
  service. No Node.js server is required on the Pi.

## Display support

Initial hardware is a monochrome Waveshare 7.5-inch e-paper HAT purchased in
March 2024. The owner recalls 800 × 400 pixels; Waveshare documents the likely
V2 model as 800 × 480. Resolution and revision remain unverified until the
hardware can be inspected. Purchase date alone does not establish revision.

Begin with this exact panel and a simulated display. Eventually support the
Waveshare e-paper models covered by its supplied driver software. Each model
requires an explicit capability profile and validation; compatibility is not
implied by using the same HAT or vendor driver repository.

Plugins express update requirements. The host can defer an update or select a
full refresh to respect panel capabilities and maintenance requirements.

Transport components should target visible updates every 30–60 seconds where
the panel supports them. Other components can update much less frequently.
Occasional full-refresh flashing is acceptable. Include configurable quiet
hours: freeze the physical display while continuing to fetch data, then resume
normal display updates afterward. The editor must distinguish the frozen
displayed content from newer available data.

## Initial integrations

| Integration | Required behaviour | Unresolved details |
| --- | --- | --- |
| Weather | Location-based conditions and forecasts | Provider and location selection |
| Google Calendar | Combine selected calendars from multiple accounts into one agenda, with source labels | Account-linking flow and display formats |
| Luas | Local stop arrival forecasts in Ireland | Stops, directions, API availability and usage limits |
| RSS | Feed headlines | Feed selection, item rotation, RSS/Atom coverage |
| Quote of the day | A daily quotation | Source and selection rules |

These are the initial product integrations, not a commitment to implement all
of them in the first engineering milestone.

The owner is comfortable supplying a Google app token. The precise credential
type and setup flow remain to be clarified: Calendar API access uses OAuth
access and refresh tokens, rather than Google account app passwords.

Calendar presentation must allow users to hide descriptions, locations, or event
titles while retaining useful availability information. Apply the same privacy
choices to the browser preview and physical display.

## Built-in time and date component

Provide a simple component using the host's system clock, without an external
API or credentials. Allow time, date, or both, with a selectable timezone,
12/24-hour time, and date formatting. Use the same presentation customisation
and placement rules as other components.

Default to minute-level time updates; date-only content changes at local date
boundaries. Physical updates remain subject to the display refresh policy and
quiet hours.

## Editor and status

- Prioritise a traditional desktop browser, including mouse and keyboard use.
  Mobile-first design and full phone/tablet editing are not initial priorities.
- The display representation is the primary UI surface.
- Default to the last known displayed content.
- Show host IP addresses and display status prominently.
- Drag, resize, place, and remove components.
- No overlapping component rectangles and no layers.
- Highlight invalid placements while dragging. On an invalid drop, restore the
  last valid position without moving neighbouring components.
- Resizing reflows content within the new bounds. Keep explicit font sizes and
  provide configurable wrapping/truncation rather than proportionally scaling
  the entire component.
- Optional grid snapping, numeric position and dimensions, keyboard movement,
  undo/redo, and explicit Save/Discard.
- Saving applies the layout to the physical display through the refresh policy.
  The UI must distinguish saved, pending, and successfully displayed content.
- Provide plugin templates and a visual component builder for arranging text,
  icons, lists, charts, and data bindings inside each component. Keep component
  rectangles non-overlapping on the display. The exact initial building blocks
  and editing interactions remain to be designed.
- Allow components to bind to any compatible dataset, regardless of which plugin
  supplies the template. For example, a generic list component can show headlines
  or transport arrivals. Dataset compatibility and field mapping need an explicit
  contract in the architecture.
- Plugin-provided setting definitions generate settings panels, including
  polling intervals, credentials, and integration-specific configuration.
- Provide a per-connection "Fetch now" action and a "Refresh display now" action.
  Fetching remains subject to provider limits; physical updates remain subject
  to the panel refresh policy and quiet hours. Fetching data and refreshing the
  display are distinct operations.

## Failure and retention

- Retain the last good component content when its source fails.
- Display a stale-data indicator within the affected component.
- Distinguish a successful empty result from a failed fetch.
- Support bounded historical data storage for future graph components, such as
  air-quality history. Air quality itself is not currently planned.
- Historical retention limits and stale thresholds remain to be selected.

## Application updates

Initially update the core application through a documented SSH command. Keep
plugin installation in the web UI as already agreed.

After the initial release, add checks for newer GitHub releases and an update
flow that installs only after explicit user approval. Specify release
authenticity/integrity checks, architecture and plugin compatibility, backup
before migrations, restart, health checking, and rollback before implementing
self-updates. Installing an update must preserve user configuration.

## Licensing policy

The owner's intent is free use and modification for non-commercial purposes,
with attribution to Odd Orbit Software. Select and apply exact licence terms
before distribution; scope across the application, SDK, examples, documentation,
and authoring skill still needs to be settled.

PolyForm Noncommercial 1.0.0 with an explicit `Required Notice:` attribution is
a software-licence candidate, not an adopted licence. Review its permitted
purposes, including its express allowances for non-commercial organisations,
against the intended policy. Record third-party dependency, asset, and data
licences separately.

## AI-assisted plugin authoring

Once the plugin API, SDK, package format, and examples are established, provide
a Codex-compatible plugin-authoring skill with tool-independent instructions
usable by other AI coding assistants.

Cover scaffolding, host services, settings and secrets, typed datasets, component
templates, refresh hints, testing, and packaging for upload or GitHub releases.
Keep the skill aligned with supported SDK/API versions and validate it by
creating, building, and loading a new example plugin through the documented
workflow. Create the skill after the contracts are worked out, rather than
teaching provisional interfaces.

## Delivery

Follow the agreed sequence in [delivery-plan.md](delivery-plan.md): validate
foundations, build the editor with a simulated display, connect the physical
display, then complete integrations and plugin distribution.

## Documentation

- Record agreed plans in `docs/planning`.
- Explain implemented features in `docs/design`.
- Keep documentation concise, descriptive, and understandable.
- Explain relevant data structures and walk through nontrivial algorithms.
- Include diagrams and actual screenshots where they clarify behaviour.
- Keep proposed designs distinct from implemented behaviour.
- Put installation and operation instructions in `docs/usage`.

## Commit and versioning policy

Use short, descriptive commit subjects and keep detailed changes and validation
in the repository's [changelog](../../changelog.md).

The current `0.1.0` foundation is experimental. Once the earliest stable stage
has been reached and recorded, increment the minor version on every commit,
including documentation-only commits. Update workspace package versions in
`Cargo.lock` with the workspace version. Define and record the earliest stable
milestone before starting this per-commit versioning policy.

## Sources and validation notes

- [Waveshare 7.5-inch manual](https://www.waveshare.com/wiki/7.5inch_e-Paper_HAT_Manual):
  panel revisions, resolution, refresh modes, and maintenance guidance.
- [Raspberry Pi Zero 2 W](https://www.raspberrypi.com/products/raspberry-pi-zero-2-w/):
  512 MB RAM and a 64-bit-capable processor; installed OS architecture is separate.
- [TII Luas forecasting dataset](https://data.gov.ie/dataset/luas-forecasting-api):
  stop-specific arrival forecasts. The listing does not prove present endpoint
  availability or establish a polling allowance.
- [Google OAuth web-server documentation](https://developers.google.com/identity/protocols/oauth2/web-server):
  web callbacks require HTTPS and cannot use raw non-loopback IP addresses.
  A browser on another machine cannot use its own localhost to reach the Pi.
- [PolyForm Noncommercial 1.0.0](https://polyformproject.org/licenses/noncommercial/1.0.0):
  candidate software terms covering permitted use, modification, redistribution,
  and preservation of required notices. No licence text has been adopted yet.
