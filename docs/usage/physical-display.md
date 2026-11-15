# Run Paperphase on the attached Waveshare V2 screen

This backend is for the confirmed monochrome 800×480 7.5-inch V2 panel with HAT
Rev2.3. Other Waveshare panels require their own driver/profile validation. The
safe `waveshare-v2` profile uses full refresh, at least three minutes apart.
Fast refresh is not enabled. A clock or frequent source may queue several changes while
only the latest is sent when the display is ready.

The Pi needs SPI0/CE0 enabled and its service user in `spi` and `gpio`. The known
image already has `python3`, `spidev`, `gpiozero` and `lgpio`. Hardware switches
remain Display Config B and Interface Config 0. Power off before changing wiring.

Install the pinned upstream `waveshare_epd` package (empty `__init__.py`,
`epdconfig.py`, `epd7in5_V2.py`) described in the
[bring-up guide](foundation-harness.md#one-shot-waveshare-v2-full-refresh).
Keep its original license headers. Paperphase verifies these files before using
GPIO. The directory passed below contains that package directory.

```sh
bin/paperphase serve --listen 0.0.0.0:8080 \
  --state-file live/workspace.json --connector-bindings live/bindings.json \
  --display waveshare-v2 --waveshare-vendor-dir /home/pmck/paperphase-test/panel
```

The default interpreter is `/usr/bin/python3`; `--display-python` can select a
compatible system interpreter. Physical mode requires the real host clock.
Keep the app's existing workspace, connections, designs and Google files together.
The new `*.display.json` recovery journal is private and should join those backups.
Do not delete it to bypass refresh timing. Stop the service before moving state.

## Headless service on the temporary Pi image

[The service example](../../deploy/paperphase-pi.service) uses the existing
`pmck` test account and `/home/pmck/paperphase-test` installation. Adapt these paths
for another installation. On the confirmed test panel it selects the explicit
partial profile described below. It runs without root, with a writable `live` directory
and temporary adapter work directories. Its shared `/tmp` hardware lock must
remain visible to the smoke utility; do not give them separate private `/tmp`
namespaces. `KillMode=mixed` lets active updates complete during graceful shutdown.

The separate [demo-source service](../../deploy/paperphase-demo-source.service)
is only for this test image's synthetic temperature connection; it is not a
physical sensor or a production dependency.

```sh
sudo systemctl enable --now paperphase
systemctl status paperphase --no-pager
journalctl -u paperphase --since today
sudo systemctl restart paperphase
```

Use `http://192.168.1.53:8080` on the current test LAN, or the Pi's actual address.
**Save to display** queues an update. Preview and editing do not write to the
screen. **Refresh display** obeys quiet hours, timing and retry limits; it is not a
way to force rapid flashing. The screen sleeps and keeps its image between updates.

Before the first successful operation, the editor says it is waiting for a screen
update. After restart it may show the last completed reference with an unconfirmed
state until full recovery succeeds. The three-minute cooldown also survives a
restart. If hardware output fails, check power/wiring/service logs; the last
completed reference remains available and the controller retries conservatively.
If a recovery-record warning appears after success, check free storage and write
permissions. Do not run the standalone smoke tool while the service owns the screen.

Physical completion is based on BUSY/driver acknowledgement, not optical readback.
Owner visual checks, partial-refresh qualification and longer ghosting/power runs
remain distinct from desktop tests.

## Developer partial-window check

`scripts/waveshare-v2-qualify.py` is an opt-in qualification tool, not an app
profile. Stop the service first and arrange a guaranteed service restart even if
the SSH session disconnects. It uses the same exclusive lock and pinned vendor
files, waits for the durable cooldown, and reserves an attempt before GPIO access.
It leaves the app's completed receipt intact and restores the three-minute
cooldown after the check. The normal service then recovers with a full refresh.

From the repository on the Pi, for an unused 160×48 rectangle at 8,385:

```sh
python3 scripts/waveshare-v2-qualify.py \
  --journal /home/pmck/paperphase-test/live/workspace.display.json \
  --vendor-dir /home/pmck/paperphase-test/panel --region 8 385 160 48
```

The tool rejects rectangles that overlap saved components. Select a free rectangle
for the actual workspace; the coordinates above are specific to the current test.
It uses standard-library bitmap markers and needs no Pillow dependency. A full
baseline shows **PART A** on white, then one partial update should show **PART B**
in white on black in that same rectangle. Outside content must stay unchanged.
BUSY/timing/cleanup reports cannot confirm that optical result. This first check
does not establish partial updates after sleep or a safe long-term cadence.

For the follow-up sleep/wake check, add `--wake-cycle`. It establishes a full
**PART A** baseline and exits that worker through sleep/power release. Four new
workers then show **PART B**, **PART A**, the original empty area, and **PART B**
again, with five-second powered-off gaps. Each worker uses partial initialization
only; no full refresh or reference RAM write is hidden in those four operations.
The whole sequence has a 130-second deadline in addition to per-worker watchdogs.
These short gaps are a qualification workload, not an approved production cadence.

## Opt-in partial updates on the confirmed panel

Use `--display waveshare-v2-partial` instead of `--display waveshare-v2` to allow
small aligned changes to update part of the screen. All updates remain at least
three minutes apart, with at most three partials before full maintenance. Large
changes (over 40% of the screen), startup and failure recovery use full refresh.
The hard maintenance age is 30 minutes, including unchanged content. The screen sleeps
and releases HAT power after every full or partial operation. Device settings
explain the connected profile; saved preferences cannot bypass these limits.

The owner visually confirmed this panel's small-window update, wake-after-sleep,
reversal, removal and re-addition checks. Other panels and faster update cadences
need separate qualification. To return to full-only behaviour, change the unit
back to `--display waveshare-v2`, reload systemd and restart Paperphase. Keep the
existing recovery journal so the cooldown survives.

## Component choices and screen-wide cleanup

Choose a component, then **Style → Display updates → Update method**. Automatic
is recommended; Partial when possible prefers a small update; Full screen requests
a whole-screen refresh when that component changes. Preferences cannot bypass
capability, area, timing, recovery or maintenance limits. Unchanged neighbouring
components do not force full refresh just by lying inside a changed bounding box.

The three-partial limit is shared across the whole display. Reaching it queues
full cleanup even if no new data arrives. The partial profile also schedules
full cleanup within the effective full interval (at most 30 minutes), including
static content. A full acknowledgement resets both age and count. The full-only
profile similarly schedules its configured full-age cleanup. Quiet hours defer
cleanup until updates resume, and the hardware's minimum spacing always applies.

Device shows effective, enforced limits rather than an unsupported request such
as ten partials on a three-partial profile. You can choose stricter values. The
30-minute value is Paperphase's conservative policy, not a vendor mandate; it is
independent of the owner's non-concrete 15-minute example.
