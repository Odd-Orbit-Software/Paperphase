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
