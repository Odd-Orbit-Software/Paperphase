#!/usr/bin/env python3
"""Write one Paperphase PBM frame using the separately supplied Waveshare driver.

This is a hardware bring-up tool, not the application's display backend. It
requires the monochrome 7.5-inch V2 panel, HAT Rev2.3, SPI0/CE0 and the vendor's
epdconfig.py / epd7in5_V2.py package. Full refresh only; always releases power.
"""

import argparse
import fcntl
import hashlib
import json
from pathlib import Path
import signal
import sys
import time


def read_frame(path):
    """Accept the exact binary PBM format emitted by Paperphase's renderer."""
    data = path.read_bytes()
    parts = data.split(b"\n", 2)
    if len(parts) != 3 or parts[:2] != [b"P4", b"800 480"]:
        raise ValueError("Expected a Paperphase P4 PBM with an 800 480 header")
    if len(parts[2]) != 48000:
        raise ValueError("Expected exactly 48,000 packed pixel bytes")
    return parts[2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pbm", type=Path, required=True)
    parser.add_argument("--vendor-dir", type=Path)
    parser.add_argument("--report", type=Path)
    parser.add_argument("--validate-only", action="store_true")
    args = parser.parse_args()
    pixels = read_frame(args.pbm)
    report = {
        "panel": "waveshare-7.5-v2-mono",
        "hat_revision": "2.3",
        "frame_sha256": hashlib.sha256(pixels).hexdigest(),
        "frame_bytes": len(pixels),
        "mode": "full",
        "busy_waits": [],
        "visual_confirmation": "pending",
    }
    if args.validate_only:
        print(json.dumps(report, indent=2))
        return
    if args.vendor_dir is None:
        parser.error("--vendor-dir is required for a physical refresh")

    # One owner for this bring-up tool. The application has no physical backend.
    with open("/tmp/paperphase-waveshare-v2.lock", "a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        config = None
        started = time.monotonic()
        success = False

        def timed_out(signum, frame):
            raise TimeoutError("Physical refresh exceeded its time budget")

        previous_alarm = signal.signal(signal.SIGALRM, timed_out)
        previous_term = signal.signal(signal.SIGTERM, timed_out)
        signal.setitimer(signal.ITIMER_REAL, 75)
        try:
            sys.path.insert(0, str(args.vendor_dir.resolve()))
            from waveshare_epd import epdconfig, epd7in5_V2

            config = epdconfig
            if (config.RST_PIN, config.DC_PIN, config.BUSY_PIN, config.PWR_PIN) != (17, 25, 24, 18):
                raise ValueError("Vendor GPIO map differs from the inspected Rev2.3 wiring")

            class BoundedPanel(epd7in5_V2.EPD):
                def ReadBusy(self):
                    wait_started = time.monotonic()
                    observed_busy = False
                    self.send_command(0x71)
                    while config.digital_read(self.busy_pin) == 0:
                        observed_busy = True
                        if time.monotonic() - wait_started >= 20:
                            raise TimeoutError("Panel BUSY stayed asserted for 20 seconds")
                        time.sleep(0.01)
                        self.send_command(0x71)
                    time.sleep(0.02)
                    report["busy_waits"].append({
                        "elapsed_ms": (time.monotonic() - wait_started) * 1000,
                        "busy_observed": observed_busy,
                    })

                def send_data2(self, data):
                    # The vendor complement plane can contain negative ints.
                    # Preserve the vendor's D/C and chip-select transitions.
                    return super().send_data2(bytes(value & 0xFF for value in data))

            panel = BoundedPanel()
            if (panel.width, panel.height) != (800, 480):
                raise ValueError("Vendor driver dimensions differ from the frame")
            panel.init()
            report["initialise_ms"] = (time.monotonic() - started) * 1000
            refresh_started = time.monotonic()
            panel.display(pixels)
            report["refresh_ms"] = (time.monotonic() - refresh_started) * 1000
            report["refresh_busy_observed"] = report["busy_waits"][-1]["busy_observed"]
            panel.sleep()
            report["sleep_completed"] = True
            success = True
        except BaseException as error:
            report["error"] = f"{type(error).__name__}: {error}"
            raise
        finally:
            try:
                if config is not None:
                    config.module_exit(cleanup=True)
                    report["gpio_released"] = True
            finally:
                signal.setitimer(signal.ITIMER_REAL, 0)
                signal.signal(signal.SIGALRM, previous_alarm)
                signal.signal(signal.SIGTERM, previous_term)
                report["command_sequence_completed"] = success
                report["elapsed_ms"] = (time.monotonic() - started) * 1000
                encoded = json.dumps(report, indent=2)
                if args.report is not None:
                    args.report.write_text(encoded + "\n")
                print(encoded, flush=True)


if __name__ == "__main__":
    main()
