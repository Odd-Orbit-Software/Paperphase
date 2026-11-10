# Delivery plan

Status: delivery order agreed. The criteria below make each stage reviewable;
they do not by themselves report completed work. Stage 1 is in progress: the
[plugin harness](../design/plugin-foundation.md) is implemented, with current
[validation results](../usage/foundation-harness.md#foundation-progress).

Product scope is in [requirements.md](requirements.md). Architectural boundaries
are in [architecture.md](architecture.md).

## 1. Validate foundations

Resolve the uncertainties that could change the architecture before investing
in the full application.

| Work | Completion evidence |
| --- | --- |
| Compiled plugin execution | A minimal plugin uses host networking and returns data on a Zero 2 W; record idle/active memory, installation peak memory/time, and execution latency |
| Resource limits | Demonstrate recovery from an infinite loop, excessive allocation, failed request, and blocked host operation without stopping healthy work |
| Google Calendar | A supported setup flow connects from another LAN machine; verify multiple accounts, least-privilege read access, refresh-token renewal, and reauthorisation behaviour |
| Luas | Verify a stop forecast response, stop/direction identifiers, permitted polling, and attribution |
| Panel identification | Confirm resolution, panel revision, driver variant, and GPIO/SPI mapping when the hardware is available |

Record measurements and constraints, then choose the runtime and define the
first plugin package/API contract. Evaluate Wasmtime first. Establish a resource
budget from measurements before declaring the runtime suitable; no memory or
latency target is claimed as already achievable.

Do not require the panel to be available for simulator/editor work. Its exact
identity is required before physical driver validation in stage 3. Desktop-only
measurements do not substitute for the plugin evaluation on a Zero 2 W.

## 2. Build the editor against a simulated display

Progress: the [shared renderer and file-based simulator](../design/data-and-rendering.md)
and the [first desktop editor/service](../design/desktop-editor.md) are
implemented, including text/data templates, a block builder, draft isolation,
non-overlapping geometry, undo/redo, durable revision-checked saves, and simulated
completion state. Plugin settings, the live clock, scheduling, and refresh policy
remain to be delivered; stage 2 is not complete.

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

## 3. Connect the physical display

Implement and validate the driver/profile for the identified panel using the
same compositor and refresh-controller interfaces as the simulator.

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

## 4. Complete integrations and plugin distribution

Finish weather, combined multi-account Google Calendar, Luas arrivals, RSS
headlines, and quote-of-the-day plugins. Early feasibility code is not a finished
integration.

- Complete setup, adjustable settings, data bindings, and useful templates.
- Complete calendar privacy settings for descriptions, locations, and event
  titles, with matching preview and physical presentation.
- Validate failure, stale-data, empty-result, and partial-source failure states.
- Provide bounded historical storage for graph-capable data without adding an
  air-quality integration to the agreed scope.
- Complete UI package upload and GitHub release installation, compatibility
  checks, activation, restart, and rollback behaviour.
- Publish the plugin API/SDK instructions and an example community plugin.
- Once the plugin contracts and examples are established, create a Codex-compatible
  plugin-authoring skill with portable instructions for other AI coding assistants.
  Cover scaffolding, shared host services, settings/secrets, datasets, templates,
  refresh hints, validation, and packaging. Identify compatible SDK/API versions.
- Complete installation, account setup, operation, backup, and recovery guidance.
- Document the initial SSH command for core application updates, including
  configuration preservation, migration backup, and recovery steps.
- Finalise the non-commercial-use and attribution licence terms and required
  notices before distributing the application and authoring resources.

Completion: demonstrate the initial integrations together, combined calendar
source labels, shared host networking, and both installation paths. Verify an
incompatible or failed plugin installation preserves a usable prior state.
Validate the authoring skill by following it to create a new example plugin,
build and package it, and load it in the simulator through the UI. Keep its
instructions and examples in step with subsequent plugin API changes.

## Later: approved application self-updates

After the initial release, check GitHub releases for newer core versions and
offer installation after explicit user approval. This follows the initial SSH
update workflow rather than replacing it in the first release.

Specify release authenticity/integrity verification, target architecture,
plugin compatibility, configuration backup/migration, restart, health checking,
and rollback. Completion must demonstrate both a successful upgrade and recovery
from a failed update without losing the prior usable configuration.

## Documentation for every implemented feature

Update `docs/design` alongside implementation. Explain behaviour, relevant data
structures, nontrivial algorithms, validation, and limitations. Include diagrams
and actual UI screenshots or panel photographs when they clarify the result.
Keep operator instructions in `docs/usage` and reconcile plans with decisions
made during implementation.
