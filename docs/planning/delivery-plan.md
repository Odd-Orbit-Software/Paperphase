# Delivery plan

Status: delivery order agreed, with initial plugin work before physical hardware.
The criteria below make each stage reviewable;
they do not by themselves report completed work. Stage 1 is in progress: the
[plugin harness](../design/plugin-foundation.md) is implemented, with current
[validation results](../usage/foundation-harness.md#foundation-progress).
Initial headless Zero 2 W execution, resource-boundary and editor/API checks
passed on 2026-11-14; [measurements](../usage/foundation-harness.md#first-headless-pi-results--2026-11-14)
record approximately 52.5 MiB peak service RSS and a 43-second startup with all
eight modules registered. Production resource budgets, longer runs, Google
account validation and physical-panel validation remain open. The owner has
identified the labels as HAT revision 2.3 / monochrome 7.5-inch screen V2;
the vendor's documented 800 × 480 resolution matches the simulator. A one-shot
full refresh was visually confirmed on 2026-11-14; integrated physical updates
and partial/maintenance/recovery behaviour remain open.

Product scope is in [requirements.md](requirements.md). Architectural boundaries
are in [architecture.md](architecture.md).

## Next agreed work

On 2026-11-14, after the first physical full-refresh test, the owner agreed the
next order: **Google Calendar plugin → component design before placement →
automatic physical-display integration**. Longer Pi runs and remaining provider
checks remain foundation work alongside these tasks.

Calendar preflight on 2026-11-15 verified the owner's hosted HTTPS callback
health and the Pi's new account setup UI with nine registered modules. Saved
layout/connections were preserved. Real Google client configuration, consent
and renewal are still pending; the next-work order above is unchanged. See
[Calendar validation](../design/google-calendar.md#pi-and-hosted-callback-preflight--2026-11-15).

### Component design before placement

The owner approved merging the UIUX redesign on 2026-11-15 and confirmed this
as the next feature. Keep its canvas/task-dock layout, Content/Position/Style
organisation, paper/green palette and separate advanced controls; follow
[DESIGN.md](../../DESIGN.md) and the
[owner workspace](../design/owner-workspace.md).

Address these owner-reported regressions as part of this work:

- Delete no longer reliably removes a highlighted display component or an
  internal element during ordinary mouse-driven editing. The current shortcut
  guard requires focus inside the canvas/list/viewport; selecting with the mouse
  can leave focus elsewhere. Restore selection-based deletion in editor context
  while preserving native Delete/Backspace in text inputs and protecting dialogs.
- The internal component UI designer is missing from the owner's workflow.
  Restore a clear, reachable entry point for existing components and reuse it for
  plugin-specific custom designs. Do not treat the presence of an advanced tab
  or a source function as proof the interactive workflow is accessible.

Add regression coverage that clicks a component/element with the mouse and then
presses Delete, verifies undo restores it, and preserves the existing last-element
constraint. Verify text-field deletion edits text rather than removing its
component. Exercise entering, editing and leaving the internal designer through
the visible UI. The earlier 26 passing browser tests did not establish these
owner-reported paths; both issues were open at merge; implementation and validation are recorded in
[the component-designer design](../design/component-designer.md).

Every loaded plugin should offer a way to design a custom component before
adding it to the display. Existing component editing does not solve the current
failure: a default template must first fit a free display rectangle, so an
oversized or unsuitable default cannot be added and therefore cannot be edited
into a usable design.

- Open an independent component draft from a plugin/connection. Start with a
  blank design or an editable copy of a supplied template; a supplied template
  must not be the only route into the designer.
- Choose dimensions, content, compatible data bindings and presentation before
  placement. Reuse the existing element-editing tools and shared host preview.
- Keep the designer accessible when a default exceeds the panel dimensions,
  overflows internally or has no free placement on the current display.
  Explain problems while allowing the user to resize, remove or rearrange
  elements and adjust typography/padding to resolve them.
- Keep this draft separate from the main layout. Editing or cancelling it must
  not alter neighbouring components, the saved layout or the completed frame.
- Add the finished component to the layout only through an explicit placement
  action. Apply the normal content, display-boundary and whole-component non-overlap validation;
  if no valid placement exists, retain the editable design and explain why.
- Support the same flow for Google Calendar and the existing plugins, including
  plugins without supplied component templates.

Completion: demonstrate an oversized default becoming a smaller valid design
before insertion, creation from a blank plugin-backed draft, and correction of
an internally overflowing template. Verify that a crowded display still allows
designing, failed placement retains the design, cancellation leaves the layout
unchanged, and an inserted component retains its bindings and can be edited and
saved normally. Document the implemented flow and actual screenshots at that
time. Implemented on `component-designer`: see [the implemented design](../design/component-designer.md)
and [owner instructions](../usage/component-designer.md). Desktop validation is
recorded with that change; Pi and physical-display validation remain separate.

## 1. Validate foundations

Resolve the uncertainties that could change the architecture before investing
in the full application.

| Work | Completion evidence |
| --- | --- |
| Compiled plugin execution | A minimal plugin uses host networking and returns data on a Zero 2 W; record idle/active memory, installation peak memory/time, and execution latency |
| Resource limits | Demonstrate recovery from an infinite loop, excessive allocation, failed request, and blocked host operation without stopping healthy work |
| Google Calendar | A supported setup flow connects from another LAN machine; verify multiple accounts, least-privilege read access, refresh-token renewal, and reauthorisation behaviour |
| Luas | Verify a stop forecast response, stop/direction identifiers, permitted polling, and attribution |
| Panel identification | HAT 2.3 / monochrome 7.5-inch screen V2, 800 × 480; current vendor V2 full refresh and SPI/GPIO mapping confirmed in a one-shot test. Profile/partial-refresh validation remains open |

Record measurements and constraints, then choose the runtime and define the
first plugin package/API contract. Evaluate Wasmtime first. Establish a resource
budget from measurements before declaring the runtime suitable; no memory or
latency target is claimed as already achievable.

Do not require the panel to be available for simulator/editor work. Its exact
identity is required before physical driver validation in stage 4. Desktop-only
measurements do not substitute for the plugin evaluation on a Zero 2 W.

## 2. Build the editor against a simulated display

Progress: the [shared renderer and file-based simulator](../design/data-and-rendering.md)
and the [first desktop editor/service](../design/desktop-editor.md) are
implemented, including text/data templates, a block builder, draft isolation,
non-overlapping geometry, undo/redo, durable revision-checked saves, and simulated
completion state. [Time/date blocks, clock/freshness deadlines, quiet hours, and
explicit simulated refresh](../design/clock-and-refresh.md) are also implemented.
[Generated connector settings and persistent connection records](../design/connector-settings.md)
are implemented. [Compiled connection execution, shared polling/backoff, cached
datasets, and Fetch now](../design/compiled-connectors.md) are exercised with the
Rust temperature example and local sources. [Direct internal text/data/clock
positioning](../design/internal-editor.md) now supports element geometry and
flow conversion. The [simulated refresh controller](../design/refresh-controller.md)
now exercises region alignment, partial/full decisions, maintenance, serialized
operations, and retries. Real provider policy/account checks,
fuller builder nodes, and validated panel profiles
remain pending; stage 2 is not complete.

Deliver a usable desktop workflow backed by the real rendering and scheduling
boundaries, with deterministic sample datasets and simulated display operations.

- Show displayed content, host addresses, status, and pending updates.
- Add, move, resize, and remove non-overlapping components.
- Provide snapping, numeric geometry, keyboard controls, undo/redo, Save/Discard,
  collision feedback, and reflow with wrapping/truncation.
- Provide templates and a visual builder with editable properties and bindings
  to compatible datasets.
- Include the built-in time and date component with timezone and formatting
  settings; exercise time/date boundaries and quiet-hours pause/resume using a
  controllable clock.
- Generate settings panels from plugin definitions.
- Provide separate "Fetch now" and "Refresh display now" controls; preserve
  provider limits, refresh policy, and quiet-hours behaviour.
- Persist layouts/settings and exercise normal, failed, stale, and quiet-hours
  states without needing the panel.

Completion: demonstrate the complete edit-save-refresh workflow and matching
host-rendered preview/frame pixels. Check geometry validation, draft isolation,
and pending/completed state handling with meaningful integration tests. Review
desktop interactions and include screenshots in implementation documentation.

## 3. Build initial plugins against the simulator

Build useful plugins before requiring the Pi or panel. The first
[weather and RSS plugins](../design/weather-and-rss.md) are implemented, including
hourly/daily forecasts, generated settings, and reusable templates.
[Plugin-owned monochrome icon packs](../design/plugin-icons.md) and editable
icon elements now support these templates. The user prioritised transport next:
[Luas stop boards](../design/luas.md) now support both directions, with Charlemont
as the first live setup. [Dublin Bus](../design/dublin-bus.md) follows with
multiple numbered stops per connection, private NTA authentication and a shared
reference cache. The Luas HTTPS response and attribution were checked; a
published numeric polling allowance remains unconfirmed. The agreed next stretch,
**air quality/pollen, daily quote and generic JSON**, now has compiled desktop
implementations, [design documentation](../design/air-quotes-json.md) and
[setup instructions](../usage/air-quotes-json.md). The
[next-plugin plan](next-plugins.md) also records the later Leave in X minutes
component. Google Calendar setup feasibility remains a parallel foundation task
when credentials are available.

- Use the shared host HTTP client, scheduler, settings, and dataset publication.
- Verify provider access, polling allowances, attribution, and error responses
  before treating an integration as supported.
- Supply useful initial component layouts and adjustable presentation settings.
- Exercise normal, stale, missing, empty, and failed-source states through the
  editor and simulated refresh controller.
- Establish any missing host API needed by these plugins before finalising the
  community package contract or writing the AI authoring skill.

Completion: at least two useful real-source plugins can be configured and shown
together in the desktop simulator, with repeatable fixture tests and operator
instructions. Hardware measurements and physical display behaviour remain
separate checks and are not implied by this milestone. Finish the remaining
integrations and distribution work in stage 5.

## 4. Connect the physical display

Implement and validate the driver/profile for the identified panel using the
same compositor and refresh-controller interfaces as the simulator.

Initial bring-up: a separate bounded Python utility drove saved layout revision
3 through the vendor V2 reference on 2026-11-14. Full refresh took 5.74 seconds,
the owner confirmed a correct image, and the panel entered sleep. This proves
one-shot full-frame output, not completion of this stage: the Rust service still
uses the simulator, and automatic physical updates are not connected yet.

- Validate full and partial refresh, alignment constraints, and unchanged frames.
- Check moved/removed component regions and batched pending updates.
- Establish maintenance limits and sleep/resume behaviour for this panel.
- Verify quiet-hours pause/resume and recovery from failed/interrupted updates.
- Verify startup on headless Raspberry Pi OS Lite and correct hardware access.

Completion: run an on-device workload with 30–60-second transport-style changes
where the panel permits them, mixed with slower components. Record refresh modes,
timings, visible ghosting, maintenance behaviour, and recovery results. Use
photographs where useful to compare the physical panel with the saved frame.

Other Waveshare panels remain separate driver-validation work; one working
panel does not establish compatibility with the whole family.

## 5. Complete integrations and plugin distribution

Finish weather, combined multi-account Google Calendar, Luas arrivals, RSS
headlines, and quote-of-the-day plugins. Early feasibility code is not a finished
integration.

- Complete setup, adjustable settings, data bindings, and useful templates.
- Later, implement the [Leave in X minutes derived component](next-plugins.md#later-leave-in-x-minutes)
  after calendar and cross-dataset dependencies are supported.
- Complete calendar privacy settings for descriptions, locations, and event
  titles, with matching preview and physical presentation.
- Validate failure, stale-data, empty-result, and partial-source failure states.
- Provide bounded historical storage for graph-capable data. Keep historical
  graph retention separate from the first air-quality implementation.
- Complete UI package upload and GitHub release installation, compatibility
  checks, activation, restart, and rollback behaviour.
- Provide UI updates for both the core application and installed plugins from
  GitHub releases, with a completed physical Updating frame before activation
  stops the service. A restart is acceptable. See the
  [update acceptance criteria](#ui-updates-from-github-releases).
- Publish the plugin API/SDK instructions and an example community plugin.
- Once the plugin contracts and examples are established, create a Codex-compatible
  plugin-authoring skill with portable instructions for other AI coding assistants.
  Cover scaffolding, shared host services, settings/secrets, datasets, templates,
  refresh hints, validation, and packaging. Identify compatible SDK/API versions.
- Complete installation, account setup, operation, backup, and recovery guidance.
- Document the SSH command for development/recovery updates when the UI cannot
  be used, including configuration preservation, migration backup and recovery.
- Finalise the non-commercial-use and attribution licence terms and required
  notices before distributing the application and authoring resources.

Completion: demonstrate the initial integrations together, combined calendar
source labels, shared host networking, and both installation paths. Verify an
incompatible or failed plugin installation preserves a usable prior state.
Validate the authoring skill by following it to create a new example plugin,
build and package it, and load it in the simulator through the UI. Keep its
instructions and examples in step with subsequent plugin API changes.

## UI updates from GitHub releases

Agreed on 2026-11-14: both the application and installed plugins must be
updatable through the web UI using their GitHub releases. This belongs to stage
5 release work, rather than being deferred until after the initial release.
The next-work order remains Google Calendar, component design before placement,
then automatic physical-display integration; updates follow within the release
and distribution work. SSH remains a development/recovery path.

- Let the user check releases from the configured application/plugin repository
  or supply a GitHub release URL. Show installed/selected versions and release
  information, and activate only after an explicit update action.
- Stage the correct release assets and validate authenticity/integrity, target
  architecture and host/plugin compatibility before replacing a working version.
- Preserve layouts, connection settings/secrets, custom component designs,
  cached datasets and provider cooldowns, with backups for migrations.
- Render an Updating maintenance message on the physical screen and confirm
  completion before stopping the service for activation. E-paper must retain
  that message during the accepted restart; browser-only progress is insufficient.
- Give activation/recovery an owner that survives replacement or failure of the
  application process. Restart, health-check, reconnect the browser and restore
  normal screen content according to saved display/quiet-hours state.
- On failed activation or startup, restore the previous usable application/plugin
  version and compatible state. Report failure and restore normal content after
  successful rollback; define interruption and unsuccessful-rollback recovery.

Completion: demonstrate one core-application update and one installed-plugin
update through the UI from GitHub releases. Observe Updating on the real panel
through a restart and normal content after recovery. Exercise failed download,
incompatible/corrupt assets, interrupted activation and a failed new version;
verify retained settings, component designs, cached data and rate-limit state,
along with a usable rollback path. This is planned work, not an implemented
update feature.

## Documentation for every implemented feature

Update `docs/design` alongside implementation. Explain behaviour, relevant data
structures, nontrivial algorithms, validation, and limitations. Include diagrams
and actual UI screenshots or panel photographs when they clarify the result.
Keep operator instructions in `docs/usage` and reconcile plans with decisions
made during implementation.
