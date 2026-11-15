# Automatic physical display integration

Branch `physical-display`; owner confirmed the Rev2.3 HAT / monochrome 7.5-inch
V2 panel is attached and available for visual checks. Deferred screen-improvement
ideas remain after hardware integration.

First integrated profile: full refresh only, at least 180 seconds between
attempts, bounded BUSY/operation waits, sleep/power release after each operation.
The pinned vendor reference already drove this panel successfully. Partial and
fast waveforms remain separate opt-in validation work; do not advertise simulator
partial timings as hardware capabilities.

Reuse the Rust compositor, desired/completed queue, monotonic spacing, quiet hours
and retry handling. Execute an embedded bounded Python adapter with the known
vendor driver/dependencies; preserve ownership locks across the service lifetime.
The browser must acknowledge completed panel operations only after success.

Persist an attempt timestamp before touching hardware and a last-completed frame
receipt after success. Restart enforces retained attempt spacing and starts with
an unknown physical baseline; no partial update is permitted before recovery.
Show an explicit pending/unknown state if no completed receipt exists. Storage
failure before a refresh prevents hardware access; after successful hardware
output, retain truthful completion while reporting a recovery-record warning.
Graceful SIGINT/SIGTERM drains the active bounded operation before exiting.

Validate desktop contracts, fake-driver transfer/failure/timeout/ownership,
quiet-hours and restart state, then the required Rust/Wasm and browser suites.
Cross-build ARM64, preserve Pi workspace/account/connection files without reading
private values, install the backend as a headless service and confirm actual
startup/save/data-driven output. Separate electrical completion from owner visual
confirmation. Record refresh times and open partial/long-run validation honestly.

## First full-refresh integration result

Implemented and validated on the attached Pi on 2026-11-15: startup and
source-driven full output, two clock updates, retained restart cooldown, full
recovery and exact original-layout restoration after removing the temporary
clock. The owner confirmed the integrated image was readable. Both headless
service units are enabled and active. See the
[measured results](../usage/foundation-harness.md#automatic-physical-display-results--2026-11-15).

Still pending: partial/fast waveforms and alignment on this panel, physical
quiet-hours pause/resume, interrupted-power recovery, and longer ghosting,
maintenance and power runs. Stage 4 remains open.

## Partial-window qualification

The full-refresh implementation is committed as `1be9327` and published on
`physical-display`. Begin qualification with a single bounded full baseline
followed by one partial rectangle, without enabling partials in the app profile.
Use an unused rectangle, with a nonzero byte-aligned X and odd Y, to expose stride
and endpoint errors. Change a white **PART A** marker to a black **PART B** marker;
everything outside that rectangle must remain unchanged. Preserve saved layouts,
connections and the app's completed receipt. Reserve the existing cooldown before
the test and restart the full-refresh service for restoration afterward.

The pinned vendor example exercises full-screen partial waveforms. Its rectangle
implementation allocates/transfers 48,000 bytes regardless of window size; the
qualification adapter crops source rows and limits transfer to the addressed
window. Fake-vendor tests verify inversion, nonzero origins, exact byte count,
outside-pixel isolation and cleanup before physical access.

Electrical completion and owner visual confirmation are separate. If the first
rectangle works, next qualify repeated changes, removal/clearing and restart from
sleep/power release. Decide partial spacing and maintenance limits using those
results; do not infer a production profile from one successful update. Keep fast
refresh and longer ghosting/power runs separate.

First electrical result: 160×48 at 8,385, full baseline 5.746 s, partial 0.418 s,
960 bytes, acknowledged BUSY/sleep/power release. The owner confirmed the marker and unchanged outside content. Normal
full-refresh restoration completed after the retained cooldown.

The next opt-in `--wake-cycle` check uses a full baseline followed by four fresh
partial-only workers, each ending in deep sleep and GPIO/power release. It tests
reversal, removal and re-addition with powered-off gaps; the normal full service
remains responsible for final recovery. No old-image RAM reconstruction is
assumed or invented. Optical results and timings must be recorded before choosing
a production profile.

The owner confirmed the slower repeated sleep/wake sequence. Added the explicit
`waveshare-v2-partial` app profile with 180-second spacing, at most three partials,
40% area limit and 30-minute full age, preserving the safe full-only profile.
The next validation observes real app changes through partial and maintenance
cycles; longer ghosting/power and interrupted-power recovery remain open.

Integrated result: the real app completed full → three partials → full maintenance
(`partial_limit`) over 12 minutes without errors or layout edits. Partial updates
took 0.491–0.497 s across 664–728×190 regions; full maintenance took 5.744 s and
reset the counter to zero. The owner confirmed both the integrated partial and
clean maintenance result. Initial partial integration and this bounded cycle are
validated; overnight/lifetime, fast, physical quiet-hours and interrupted-power
checks remain open.
