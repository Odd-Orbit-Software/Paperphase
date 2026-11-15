# Run Paperphase on the attached Waveshare V2 screen

This backend is for the confirmed monochrome 800×480 7.5-inch V2 panel with HAT
Rev2.3. Other Waveshare panels require their own driver/profile validation. The
current profile uses full refresh, at least three minutes apart; partial and fast
modes are not enabled. A clock or frequent source may queue several changes while
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
for another installation. It runs without root, with a writable `live` directory
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
