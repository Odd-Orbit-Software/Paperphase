# Paperphase

A configurable e-paper information display by
[Odd Orbit Software](https://oddorbit.software).

Paperphase is being developed as a Rust service for Raspberry Pi, with sandboxed,
compiled WebAssembly plugins and a desktop browser editor. Arrange and customise
components on a preview of the physical display, then save the layout to apply
it through the panel's refresh policy.

## Planned features

- Non-overlapping components with drag, resize, snapping, keyboard controls,
  undo/redo, and a visual component builder.
- Weather, a combined multi-account Google Calendar agenda, Luas arrivals,
  RSS headlines, daily quotations, and a built-in time/date component.
- Shared host networking, scheduling, rendering, and storage for plugins.
- Plugin installation through uploads or GitHub release URLs, with generated
  settings panels and reusable component templates.
- Panel-aware partial/full refresh, quiet hours, and stale-data indicators.
- Prominent host addresses, display status, and pending-update information.

## Status and plans

Foundation implementation includes a compiled-plugin CLI harness, shared typed
data/layout contracts, a monochrome renderer with PNG/PBM simulator exports,
and a desktop layout editor served by Rust with durable saves and a simulated
display. Built-in time/date blocks, clock/freshness scheduling, and quiet hours
work in the simulator. Connector definitions generate persistent settings forms
with private-value handling. Registered compiled connectors now fetch and cache
data through shared host networking, with polling, backoff, and Fetch now.
The internal editor supports direct positioning of text, data, clock and icon elements
inside each component, alongside the original vertical flow.
The simulator now exercises partial/full refresh decisions, batching, maintenance
thresholds, and serialized completion/retry behaviour. Initial real-source plugin
work runs before physical hardware. The first weather/RSS plugins now include
hourly/daily forecasts, plugin-owned monochrome icons, feed headlines, and reusable
connection templates. Luas stop boards now support both directions and shared
polling; air quality remains planned as a separate plugin.
Remaining integrations, Pi measurements, and hardware validation are pending.
The initial target is a Raspberry Pi Zero 2 W running 64-bit Raspberry
Pi OS Lite with a Waveshare 7.5-inch monochrome e-paper HAT. The exact panel
revision and resolution remain to be confirmed.

- [Requirements](docs/planning/requirements.md)
- [Architecture](docs/planning/architecture.md)
- [Delivery plan](docs/planning/delivery-plan.md)
- [Run and validate the foundation harness](docs/usage/foundation-harness.md)
- [Implemented plugin foundation](docs/design/plugin-foundation.md)
- [Run the display simulator](docs/usage/simulator.md)
- [Shared data and renderer design](docs/design/data-and-rendering.md)
- [Run the desktop editor](docs/usage/desktop-editor.md)
- [Desktop editor design](docs/design/desktop-editor.md)
- [Internal element editor](docs/design/internal-editor.md)
- [Clock and refresh scheduling](docs/design/clock-and-refresh.md)
- [Refresh controller design](docs/design/refresh-controller.md)
- [Configure simulated refresh policy](docs/usage/refresh-controller.md)
- [Configure connections](docs/usage/connections.md)
- [Connector settings design](docs/design/connector-settings.md)
- [Run a compiled connector](docs/usage/compiled-connectors.md)
- [Compiled connection design](docs/design/compiled-connectors.md)
- [Set up weather and RSS](docs/usage/weather-and-rss.md)
- [Weather/RSS design](docs/design/weather-and-rss.md)
- [Set up Luas departures](docs/usage/luas.md)
- [Luas design](docs/design/luas.md)
- [Plugin-owned icon design](docs/design/plugin-icons.md)
- [Use or replace plugin icons](docs/usage/plugin-icons.md)
- [Changelog](changelog.md)

Implemented features will be documented in `docs/design`, with setup and
operation instructions in `docs/usage`.

## Licensing

The intended terms permit non-commercial use and modification with attribution
to Odd Orbit Software. Exact licence terms are still being selected; see the
[licensing policy](docs/planning/requirements.md#licensing-policy).
