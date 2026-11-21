#!/usr/bin/env python3
"""Build signed Paperphase release packages; private keys are never copied out."""
import argparse
import hashlib
import json
import shutil
import subprocess
import tempfile
from pathlib import Path


def digest(path):
    value = hashlib.sha256()
    with path.open('rb') as source:
        for chunk in iter(lambda: source.read(65536), b''):
            value.update(chunk)
    return value.hexdigest()


def package(args, target, files, name):
    destination = args.output / name
    if destination.exists():
        raise ValueError(f'Release asset already exists: {destination.name}')
    with tempfile.TemporaryDirectory(prefix='paperphase-package-') as directory:
        directory = Path(directory)
        entries = []
        for filename, source in files:
            if source.is_symlink() or not source.is_file():
                raise ValueError(f'Package source must be a regular file: {source.name}')
            shutil.copyfile(source, directory / filename)
            entries.append({'name': filename, 'bytes': source.stat().st_size, 'sha256': digest(directory / filename)})
        root = Path(__file__).resolve().parents[1]
        notices = []
        if target['kind'] == 'application':
            notices.append(root / 'assets/fonts/LICENSE-DejaVu.txt')
        elif target.get('connector_id') == 'weather.open-meteo':
            notices.append(root / 'plugins/weather/assets/meteocons/LICENSE')
        else:
            names = {'transport.luas': 'luas', 'transport.dublin-bus': 'dublin-bus',
                     'environment.air-quality': 'air-quality', 'daily.quote': 'daily-quote'}
            folder = names.get(target.get('connector_id'))
            if folder:
                notices.append(root / 'plugins' / folder / 'NOTICE.md')
        if notices:
            notice = directory / 'NOTICE.txt'
            notice.write_text('Paperphase — Odd Orbit Software\n\n' + '\n\n'.join(path.read_text() for path in notices))
            entries.append({'name': 'NOTICE.txt', 'bytes': notice.stat().st_size, 'sha256': digest(notice)})
        manifest = {'schema_version': 1, 'update_protocol': 1, 'state_schema': 1,
                    'repository': args.repository, 'release': args.release,
                    'version': args.version, 'build': args.build,
                    'target': target, 'files': entries}
        manifest_file = directory / 'manifest.json'
        manifest_file.write_text(json.dumps(manifest, indent=2) + '\n')
        subprocess.run([str(args.agent), 'pack', '--manifest', str(manifest_file),
                        '--source', str(directory), '--key', str(args.private_key),
                        '--key-id', args.key_id, '--output', str(destination)], check=True)
        (args.output / (name + '.manifest.json')).write_text(json.dumps(manifest, indent=2) + '\n')
    return destination


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--agent', type=Path, required=True, help='Native signing tool executable')
    parser.add_argument('--private-key', type=Path, required=True)
    parser.add_argument('--key-id', required=True)
    parser.add_argument('--repository', default='Odd-Orbit-Software/Paperphase')
    parser.add_argument('--release', required=True)
    parser.add_argument('--version', default='0.1.0')
    parser.add_argument('--build', required=True, help='Source commit/build identity')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--application', type=Path)
    parser.add_argument('--platform', help='Rust target triple for the application')
    parser.add_argument('--plugin', action='append', default=[], metavar='CONNECTOR=MODULE')
    parser.add_argument('--icons', action='append', default=[], metavar='CONNECTOR=ASSET_DIRECTORY')
    args = parser.parse_args()
    if not args.application and not args.plugin:
        parser.error('Choose at least one application or plugin payload.')
    if args.application and not args.platform:
        parser.error('--application requires --platform')
    args.agent = args.agent.resolve()
    args.private_key = args.private_key.resolve()
    args.output.mkdir(parents=True, exist_ok=True)
    icons = dict(value.split('=', 1) for value in args.icons)
    results = []
    if args.application:
        results.append(package(args, {'kind': 'application', 'platform': args.platform},
                               [('paperphase', args.application)], f'paperphase-{args.platform}.ppkg'))
    for item in args.plugin:
        connector, module = item.split('=', 1)
        files = [('plugin.wasm', Path(module))]
        if connector in icons:
            assets = Path(icons[connector])
            files.extend((name, assets / name) for name in ['icons.json', 'icons-16.png', 'icons-24.png', 'icons-32.png', 'icons-48.png', 'icons-64.png'] if (assets / name).exists())
        results.append(package(args, {'kind': 'plugin', 'connector_id': connector, 'abi': 'paperphase_v0'}, files, f'{connector}.ppkg'))
    checksums = ''.join(f'{digest(path)}  {path.name}\n' for path in results)
    (args.output / 'SHA256SUMS').write_text(checksums)
    print(f'Created {len(results)} signed packages. Signing keys were not included.')


if __name__ == '__main__':
    main()
