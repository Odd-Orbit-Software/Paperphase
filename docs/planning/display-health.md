# Component update preferences and automatic screen cleanup

The owner approved committing partial integration, then making component choices
and global screen-health enforcement work together. Example timing such as
15 minutes is not a prescribed requirement. Baseline partial integration is
published as `17798c1`; foundation version remains 0.1.0.

Expose each component's existing Automatic / Partial when possible / Full screen
preference without an Advanced disclosure. Preferences apply to actual changed
pixels in that component; an unchanged full-preferring component must not force
full refresh merely because it lies inside a union bounding rectangle.

The physical profile owns unbypassable cleanup limits. Retain the tested maximum
of three successful partials, 180-second hardware spacing/retry, 40% partial area,
and conservative 30-minute full age for the partial-capable profile. These are
Paperphase's conservative policy values, not a vendor-defined 15/30-minute rule.
The [Waveshare manual](https://www.waveshare.com/wiki/7.5inch_e-Paper_HAT_Manual)
advises full cleanup after several partials, sleep/power-off between operations,
and regular full refresh. Do not relax the qualified cadence in this change.

Make count/time cleanup independent of content changes: a static screen still
gets a full refresh when due. Count and age reset only after acknowledged full
completion. An owner-requested long update interval cannot hold overdue cleanup;
hardware spacing, retry and quiet hours remain enforced. Quiet hours defer cleanup
until updates resume. Restart still requires a full recovery.

Verify static content, count/time limits, mixed component preferences, pending
worker wakeups, failure recovery and quiet-hours resume with desktop tests. Keep
Pi/GPIO evidence separate. Review the actual desktop/mobile settings captures
against the current paper/green design before finishing.

## Result

Implemented with 192 Rust tests, 11 adapter tests and 39 browser workflows passing,
plus the ARM64 release build. Desktop tests explicitly cover static cleanup,
count-only cleanup, unrelated full-preferring neighbours, long-throttle wakeup,
failed cleanup recovery and quiet-hours defer/resume. The UX reviewer returned
`ship` for the controls and the owner confirmed the wording/placement.

The deployed Pi reports scheduled cleanup enabled, three-partial and 30-minute
limits, and completed full startup recovery with no error while preserving
revision 5. Both service units remain enabled and active. The 30-minute static
wall-clock interval was tested through accelerated desktop state/time control;
that evidence is distinct from Pi startup and the earlier 12-minute mixed cycle.
