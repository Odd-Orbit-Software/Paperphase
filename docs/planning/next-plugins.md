# Next plugin stretch

Agreed order: **air quality and pollen → daily quote → generic JSON**. Develop
against the desktop simulator first. Keep the shared HTTP pool, scheduling,
secrets, stale indicators and editable templates; validate Pi performance later.

The three plugins below now have implementations. The next agreed order is
**Google Calendar plugin → component design before placement → automatic
physical-display integration**. The
[component-designer plan](delivery-plan.md#component-design-before-placement)
addresses defaults that cannot be added because they do not fit, and requires a
blank or template-based design flow for every loaded plugin before placement.

## Air quality and pollen

Use Open-Meteo's air-quality API with current European AQI, pollutant values,
hourly forecasts and pollen counts. Settings include coordinates, location label,
forecast length and pollen focus. Provide air-quality and pollen templates plus
typed datasets for custom layouts. Identify model estimates, preserve missing
values (including seasonal/regional pollen gaps), and include CAMS/Open-Meteo
attribution. Historical storage and charts remain a separate feature.

## Daily quote

Provide a quote, author and daily selection with a readable editable template.
Use an online quote provider, with an optional list of user-supplied quotes.
Offer provider-only, personal-only and mixed daily selection. Keep provider
attribution with displayed online content; do not bundle a third-party quote
collection. Use the host's clock and timezone rather than allowing arbitrary
guest access to the OS.

## Generic JSON

Fetch one explicitly configured HTTP(S) endpoint through the host. Map bounded
JSON objects/arrays into typed display fields using JSON Pointers; no scripting
or expression evaluation. Support optional private authentication headers,
configurable polling and normal stale/error handling. Make field mappings clear
in the settings UI and provide useful summary/list templates. Missing values
must stay distinct from zero, false and empty successful responses.

## Google Calendar

The agreed browser-only setup uses a hosted HTTPS callback at
`auth.paperphase.oddorbit.software`, deployed by the owner to their existing
Docker/Caddy VPS. Each Pi creates a one-use state and retains the PKCE verifier;
the relay redirects the browser to that editor's actual LAN address. Separate
Pis and accounts need no common hostname or inbound router configuration.
Desktop implementation now covers linking, private tokens, read-only event
fetching, privacy controls and a combined agenda. See
[design](../design/google-calendar.md) and [setup](../usage/google-calendar.md).
The owner's hosted callback and Pi account setup are deployed. On 2026-11-15,
after enabling Calendar API for the OAuth project, the owner completed a real
login; the Pi reported one linked account and no linking/reauthorisation error.
Multi-account Calendar fetching, token renewal and physical rendering remain
separate checks; do not count mocked flows as live account validation.

### Public Google account setup

For public distribution, the owner agreed that normal users should connect
through a shared Paperphase OAuth project without creating their own Cloud
project or supplying client JSON. Keep the shared Web client secret on the
owner-operated authentication service, outside source and Pi distributions.
Extend the hosted service for authenticated HTTPS code exchange/token renewal;
keep per-user tokens on each Pi and disclose the service's transient token
handling and availability dependency. Google production verification, branding
and a privacy policy are release work. The current stateless relay does not yet
provide this shared-client service. Retain private client/JSON setup as an
advanced self-hosting option.

### Later: account-linking UX and client JSON import

The owner requested these follow-ups after the first successful real login:

- Improve setup/linking guidance, progress, failure recovery and the transition
  from a linked account to calendar selection. Explain the Cloud API enablement
  step at the point where it is useful.
- Accept Google's downloaded Web application client JSON in the editor, validate
  its client ID/secret and redirect configuration, and let the owner review and
  explicitly save it. Keep private values masked; retain the existing safeguards
  against changing client identity while accounts are linked.
- Keep the real file out of source fixtures, bundles, logs, browser persistence
  and the public relay. Store accepted credentials through the existing private
  host configuration. Exercise the importer with synthetic JSON; the owner can
  import their real file directly without providing its contents to the agent.
- Git ignore rules do not enforce assistant access restrictions. Keep real
  credential JSON outside the checkout to reduce accidental inclusion; use
  enforced filesystem/sandbox restrictions when an actual access boundary is
  needed. An outside-project path alone is not such a boundary.

This records later work; no JSON importer or UX redesign is implemented yet.

## Later: Leave in X minutes

Build this after calendar integration and a host facility for derived datasets.
Combine an upcoming event, selected transport departure, journey duration and
walking/boarding buffer. Show the event, chosen departure and calculated
leave-by time; do not fetch providers again or add another HTTP client.

- Read existing snapshots with explicit dependencies and observation times.
- Let the user select calendar/events, stop/direction/route, destination and
  journey/walking/boarding buffers. Routing APIs are optional future inputs;
  initial travel durations can be configured.
- Select a departure that reaches the event after allowing for journey and
  arrival buffers. Calculate `leave_by = departure - walking - boarding_buffer`;
  explain missed/unsuitable connections instead of clamping a negative result
  to a misleading zero. Reject circular derived-data dependencies.
- Recompute on dependency changes and clock deadlines, subject to normal display
  refresh/quiet-hours policy.
- Preserve scheduled-versus-live labels. Mark timetable-based results as
  scheduled estimates, with stale/missing inputs identified separately. Handle
  cancellations, missed departures, all-day events, midnight and timezone changes;
  do not present a guaranteed connection.
- Keep account credentials within source connectors. Explain the selected event,
  departure and buffers so the user can understand the calculation.

Document each implemented feature in `docs/design` with useful diagrams and
actual screenshots, and operator setup in `docs/usage`. The three plugins have desktop implementations in
[air-quotes-json.md](../design/air-quotes-json.md); Leave in X minutes remains
planned, and Pi/display validation remains pending.
