#!/usr/bin/env python3
"""Bounded full/partial adapter for the pinned Waveshare 7.5-inch V2 reference.

Paperphase owns the shared hardware lock, inherited as stdin so it also survives
an abrupt parent exit until this worker releases GPIO. No account/provider data
is accepted here, only a private 800x480 PBM. Vendor code is supplied separately.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import sys
import time


def read_frame(path):
    raw = path.read_bytes()
    header = b'P4\n800 480\n'
    if not raw.startswith(header) or len(raw) != len(header) + 48000:
        raise ValueError('Expected an exact 800x480 Paperphase PBM')
    return raw[len(header):]


def crop_frame(pixels, region):
    x, y, width, height = region
    if (x < 0 or y < 0 or width <= 0 or height <= 0
            or x % 8 or width % 8 or x + width > 800 or y + height > 480):
        raise ValueError('Partial rectangle must be in bounds and byte aligned')
    return b''.join(pixels[row * 100 + x // 8:row * 100 + (x + width) // 8]
                    for row in range(y, y + height))


def validate_partial(before, after, region):
    crop = crop_frame(after, region)
    x, y, width, height = region
    reconstructed = bytearray(before)
    for row in range(height):
        start = (y + row) * 100 + x // 8
        reconstructed[start:start + width // 8] = crop[row * (width // 8):(row + 1) * (width // 8)]
    if bytes(reconstructed) != after or before == after:
        raise ValueError('Qualification must change only the requested rectangle')
    return crop


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--vendor-dir', type=Path, required=True)
    parser.add_argument('--pbm', type=Path)
    parser.add_argument('--parent', type=int, required=True)
    parser.add_argument('--power-off-only', action='store_true')
    parser.add_argument('--partial-after', type=Path,
                        help='Qualification only: full baseline followed by one partial update')
    parser.add_argument('--partial-only', action='store_true',
                        help='Wake partial from a known reference without a full refresh')
    parser.add_argument('--region', type=int, nargs=4, metavar=('X', 'Y', 'WIDTH', 'HEIGHT'))
    args = parser.parse_args()
    started = time.monotonic()
    report = {'ok': False, 'mode': 'full', 'sleep_completed': False,
              'power_released': False, 'busy_waits': []}
    config = None
    try:
        pixels = None
        partial = None
        if (bool(args.partial_after) != bool(args.region)
                or (args.power_off_only and (args.partial_after or args.partial_only))
                or (args.partial_only and not args.partial_after)):
            raise ValueError('Partial qualification requires a target frame and rectangle')
        if not args.power_off_only:
            if args.pbm is None:
                raise ValueError('Frame is required')
            pixels = read_frame(args.pbm)
            report['frame_sha256'] = hashlib.sha256(pixels).hexdigest()
            if args.partial_after:
                target = read_frame(args.partial_after)
                partial = validate_partial(pixels, target, args.region)
                report.update(mode='partial' if args.partial_only else 'full-then-partial-qualification',
                              target_sha256=hashlib.sha256(target).hexdigest(),
                              region=args.region, visual_confirmation='pending')
        def interrupted(signum, frame):
            raise TimeoutError('Display operation interrupted or timed out')
        signal.signal(signal.SIGALRM, interrupted)
        signal.signal(signal.SIGTERM, interrupted)
        signal.signal(signal.SIGINT, interrupted)
        signal.setitimer(signal.ITIMER_REAL, 75 if pixels is not None else 8)
        os.environ['GPIOZERO_PIN_FACTORY'] = 'lgpio'
        sys.path.insert(0, str(args.vendor_dir))
        from waveshare_epd import epdconfig
        config = epdconfig
        if (config.RST_PIN, config.DC_PIN, config.BUSY_PIN, config.PWR_PIN) != (17, 25, 24, 18):
            raise ValueError('Unexpected GPIO map')
        if args.power_off_only:
            report['ok'] = True
            return report
        from waveshare_epd import epd7in5_V2
        class Panel(epd7in5_V2.EPD):
            partial_bytes = None
            partial_payloads = 0
            def ReadBusy(self):
                wait_started = time.monotonic()
                observed = False
                self.send_command(0x71)
                while config.digital_read(self.busy_pin) == 0:
                    observed = True
                    if os.getppid() != args.parent:
                        raise RuntimeError('Display owner exited')
                    if time.monotonic() - wait_started >= 20:
                        raise TimeoutError('Panel BUSY exceeded 20 seconds')
                    time.sleep(0.01)
                    self.send_command(0x71)
                time.sleep(0.02)
                report['busy_waits'].append({'busy_observed': observed,
                    'elapsed_ms': round((time.monotonic() - wait_started) * 1000, 2)})
            def send_data2(self, data):
                if self.partial_bytes is not None:
                    # The pinned vendor method pads every rectangle to 48 kB.
                    # Send exactly its addressed window, retaining vendor CS/DC.
                    if len(data) != 48000:
                        raise ValueError('Unexpected pinned partial payload size')
                    data = data[:self.partial_bytes]
                    self.partial_payloads += 1
                return super().send_data2(bytes(value & 255 for value in data))
        panel = Panel()
        if (panel.width, panel.height) != (800, 480):
            raise ValueError('Unexpected panel dimensions')
        if os.getppid() != args.parent:
            raise RuntimeError('Display owner exited')
        if not args.partial_only:
            if panel.init() != 0:
                raise RuntimeError('Panel initialization failed')
            refresh_started = time.monotonic()
            panel.display(pixels)
            report['refresh_ms'] = round((time.monotonic() - refresh_started) * 1000, 2)
            if not report['busy_waits'][-1]['busy_observed']:
                raise RuntimeError('Panel never asserted BUSY during refresh')
        if partial is not None:
            if os.getppid() != args.parent:
                raise RuntimeError('Display owner exited')
            if panel.init_part() != 0:
                raise RuntimeError('Partial initialization failed')
            panel.partial_bytes = len(partial)
            x, y, width, height = args.region
            partial_started = time.monotonic()
            panel.display_Partial(partial, x, y, x + width, y + height)
            report['partial_refresh_ms'] = round((time.monotonic() - partial_started) * 1000, 2)
            if panel.partial_payloads != 1 or not report['busy_waits'][-1]['busy_observed']:
                raise RuntimeError('Partial transfer or BUSY acknowledgement missing')
            report['partial_payload_bytes'] = len(partial)
            panel.partial_bytes = None
        panel.sleep()
        report['sleep_completed'] = True
        report['ok'] = True
    except BaseException as error:
        report['error'] = type(error).__name__ + ': ' + str(error)
    finally:
        try:
            if config is not None:
                config.module_exit(cleanup=True)
                report['power_released'] = True
        except BaseException as error:
            report['ok'] = False
            report['error'] = 'GPIO cleanup failed: ' + type(error).__name__
        signal.setitimer(signal.ITIMER_REAL, 0)
        report['elapsed_ms'] = round((time.monotonic() - started) * 1000, 2)
        print(json.dumps(report), flush=True)
    return report


if __name__ == '__main__':
    sys.exit(0 if main()['ok'] else 1)
