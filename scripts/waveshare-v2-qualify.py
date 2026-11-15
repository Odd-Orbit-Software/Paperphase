#!/usr/bin/env python3
"""One opt-in full-baseline/partial-window check; the service must be stopped.

Keeps the app's private completed receipt intact, reserves its cooldown before
hardware access, and leaves restoration to the normal full-refresh service.
"""
import argparse
import base64
import fcntl
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time

PINNED = {
    '__init__.py': 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855',
    'epdconfig.py': 'e53e361f5061b3a62ca143001e20afc530593035556addd57ecd4dbf815ec9f3',
    'epd7in5_V2.py': 'fafc41091c47d2ea367c3e85d84dca437e42370451cae5f5424b72b4614b55f2',
}
GLYPHS = {
    'P': ['11110', '10001', '10001', '11110', '10000', '10000', '10000'],
    'A': ['01110', '10001', '10001', '11111', '10001', '10001', '10001'],
    'R': ['11110', '10001', '10001', '11110', '10100', '10010', '10001'],
    'T': ['11111', '00100', '00100', '00100', '00100', '00100', '00100'],
    'B': ['11110', '10001', '10001', '11110', '10001', '10001', '11110'],
    ' ': ['00000'] * 7,
}


def frames(journal, region):
    if journal.get('schema_version') != 1 or not journal.get('completed'):
        raise ValueError('An existing completed physical frame is required')
    completed = journal['completed']
    layout = completed['layout']
    if (layout['display']['width'], layout['display']['height']) != (800, 480):
        raise ValueError('Expected an 800x480 completed frame')
    pixels = base64.b64decode(completed['bits'], validate=True)
    if len(pixels) != 48000:
        raise ValueError('Invalid completed frame size')
    x, y, width, height = region
    if (x < 0 or y < 0 or x % 8 or width % 8 or width < 128 or height < 40
            or x + width > 800 or y + height > 480):
        raise ValueError('Check rectangle must fit, align to 8 pixels and be at least 128x40')
    for component in layout['components']:
        r = component['rect']
        if (x < r['x'] + r['width'] and x + width > r['x']
                and y < r['y'] + r['height'] and y + height > r['y']):
            raise ValueError('Check rectangle must be outside saved components')

    def stamp(after):
        image = bytearray(pixels)

        def dot(px, py, black):
            offset = py * 100 + px // 8
            bit = 128 >> (px % 8)
            if black:
                image[offset] |= bit
            else:
                image[offset] &= 255 ^ bit

        for py in range(y, y + height):
            for px in range(x, x + width):
                border = px in (x, x + width - 1) or py in (y, y + height - 1)
                dot(px, py, bool(after) != border)
        for index, character in enumerate('PART B' if after else 'PART A'):
            for row, line in enumerate(GLYPHS[character]):
                for column, value in enumerate(line):
                    if value == '1':
                        for dy in range(3):
                            for dx in range(3):
                                dot(x + 8 + index * 18 + column * 3 + dx,
                                    y + 8 + row * 3 + dy, not after)
        return b'P4\n800 480\n' + image

    return stamp(False), stamp(True)


def persist(path, record):
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as output:
        temporary = Path(output.name)
        try:
            output.write((json.dumps(record) + '\n').encode())
            output.flush()
            os.fsync(output.fileno())
            os.replace(temporary, path)
            directory = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
            try:
                os.fsync(directory)
            finally:
                os.close(directory)
        finally:
            temporary.unlink(missing_ok=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--journal', type=Path, required=True)
    parser.add_argument('--vendor-dir', type=Path, required=True)
    parser.add_argument('--adapter', type=Path,
                        default=Path(__file__).resolve().parents[1] / 'crates/paperphase/src/display/waveshare_v2.py')
    parser.add_argument('--region', type=int, nargs=4, required=True)
    parser.add_argument('--wake-cycle', action='store_true',
                        help='Full baseline, then four partials in separate power-releasing processes')
    parser.add_argument('--step-delay', type=int, default=5, choices=range(5, 21),
                        metavar='5..20', help='Powered-off observation gap between wake-cycle steps')
    args = parser.parse_args()
    args.journal = args.journal.resolve()
    args.adapter = args.adapter.resolve(strict=True)
    args.vendor_dir = args.vendor_dir.resolve(strict=True)
    for name, expected in PINNED.items():
        actual = hashlib.sha256((args.vendor_dir / 'waveshare_epd' / name).read_bytes()).hexdigest()
        if actual != expected:
            raise ValueError('Vendor package differs from pinned reference')

    def interrupted(signum, frame):
        raise KeyboardInterrupt('Qualification interrupted')

    signal.signal(signal.SIGTERM, interrupted)
    with open('/tmp/paperphase-waveshare-v2.lock', 'a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        journal = json.loads(args.journal.read_text())
        before, after = frames(journal, args.region)
        deadline = journal['not_before']
        if type(deadline) is not int or deadline < 0 or deadline > time.time() + 300:
            raise ValueError('Invalid or unexpectedly long persisted cooldown')
        remaining = max(0, deadline - time.time())
        print(json.dumps({'waiting_seconds': round(remaining)}), file=sys.stderr, flush=True)
        while time.time() < deadline:
            time.sleep(max(0, min(1, deadline - time.time())))
        # Bound the complete multi-worker check, including gaps, to 130 seconds.
        # This is independent of each child's existing 80-second watchdog.
        signal.signal(signal.SIGALRM, interrupted)
        signal.setitimer(signal.ITIMER_REAL, 130)
        journal['not_before'] = int(time.time()) + 390
        persist(args.journal, journal)
        with tempfile.TemporaryDirectory(prefix='paperphase-partial-') as folder:
            root = Path(folder)
            baseline, target = root / 'before.pbm', root / 'after.pbm'
            baseline.write_bytes(before)
            target.write_bytes(after)
            original = root / 'original.pbm'
            original.write_bytes(b'P4\n800 480\n' + base64.b64decode(journal['completed']['bits'], validate=True))
            common = [sys.executable, '-I', '-B', str(args.adapter),
                      '--vendor-dir', str(args.vendor_dir), '--parent', str(os.getpid())]
            def invoke(previous, desired=None, wake=False):
                report = None
                expected = desired or previous
                command = common + ['--pbm', str(previous)]
                if desired:
                    command += ['--partial-after', str(desired), '--region', *map(str, args.region)]
                if wake:
                    command += ['--partial-only']
                try:
                    with tempfile.TemporaryFile() as output:
                        result = subprocess.run(command, stdin=lock, stdout=output, cwd=root, timeout=80)
                        output.seek(0)
                        encoded = output.read(32769)
                        if len(encoded) > 32768:
                            raise ValueError('Adapter report exceeded limit')
                        report = json.loads(encoded)
                        if (result.returncode or not report.get('ok')
                                or not report.get('sleep_completed') or not report.get('power_released')
                                or report.get('target_sha256' if desired else 'frame_sha256')
                                != hashlib.sha256(expected.read_bytes()[11:]).hexdigest()):
                            raise RuntimeError('Qualification did not acknowledge completion')
                        if desired and (report.get('region') != args.region or report.get('partial_payload_bytes') != args.region[2] * args.region[3] // 8):
                            raise RuntimeError('Qualification rectangle acknowledgement differs')
                        return report
                finally:
                    if not report or not report.get('power_released'):
                        subprocess.run(common + ['--power-off-only'], stdin=lock,
                                       stdout=subprocess.DEVNULL, cwd=root, timeout=10)
            try:
                if args.wake_cycle:
                    steps = [invoke(baseline)]
                    print(json.dumps({'stage': 'full-baseline-asleep', 'report': steps[-1]}), flush=True)
                    previous = baseline
                    for index, desired in enumerate([target, baseline, original, target]):
                        # Idle between processes with HAT power released, never high voltage.
                        time.sleep(args.step_delay)
                        steps.append(invoke(previous, desired, wake=True))
                        print(json.dumps({'stage': f'wake-partial-{index + 1}', 'report': steps[-1]}), flush=True)
                        previous = desired
                    report = {'ok': True, 'mode': 'full-plus-four-wake-partials', 'steps': steps,
                              'visual_confirmation': 'pending', 'region': args.region}
                else:
                    report = invoke(baseline, target)
            finally:
                # Preserve the app's completed record; a restart must recover in full.
                journal['not_before'] = int(time.time()) + 180
                persist(args.journal, journal)
                signal.setitimer(signal.ITIMER_REAL, 0)
            print(json.dumps(report), flush=True)


if __name__ == '__main__':
    main()
