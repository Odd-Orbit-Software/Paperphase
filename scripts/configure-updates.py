#!/usr/bin/env python3
"""Create a supervisor configuration without changing existing application data."""
import argparse
import base64
import json
import os
from pathlib import Path


LABELS = {'weather.open-meteo': 'Open-Meteo weather', 'rss.headlines': 'RSS headlines',
          'calendar.google': 'Google Calendar', 'transport.luas': 'Luas',
          'transport.dublin-bus': 'Dublin Bus', 'environment.air-quality': 'Air quality',
          'daily.quote': 'Daily quote', 'generic.json': 'JSON data',
          'example.temperature': 'Temperature connector'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--data-dir', type=Path, required=True)
    parser.add_argument('--application', type=Path, required=True)
    parser.add_argument('--working-dir', type=Path, default=Path.cwd())
    parser.add_argument('--platform', required=True)
    parser.add_argument('--health-url', default='http://127.0.0.1:8080/api/update-health')
    parser.add_argument('--status-port', type=int, default=8082)
    parser.add_argument('--repository', default='Odd-Orbit-Software/Paperphase')
    parser.add_argument('--publisher', action='append', default=[], metavar='KEY_ID=BASE64_PUBLIC_KEY')
    parser.add_argument('--bindings', type=Path)
    parser.add_argument('app_args', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    arguments = args.app_args[1:] if args.app_args[:1] == ['--'] else args.app_args
    if not arguments or arguments[0] != 'serve':
        parser.error('Supply the original serve arguments after --')
    keys = dict(value.split('=', 1) for value in args.publisher)
    if not keys:
        defaults = json.loads((Path(__file__).resolve().parents[1] / 'deploy/publisher-keys.json').read_text())
        if args.repository != defaults['repository']:
            parser.error('A custom repository requires its own explicit publisher keys.')
        keys = defaults['trusted_keys']
    if any(len(base64.b64decode(value, validate=True)) != 32 for value in keys.values()):
        parser.error('Publisher keys must contain exactly 32 public bytes.')
    root, data = args.root.resolve(), args.data_dir.resolve()
    if root == data or root in data.parents or data in root.parents:
        parser.error('Managed installation and private data must be separate folders.')
    if not data.is_dir() or not args.application.is_file():
        parser.error('Existing private data and application executable must exist.')
    def target(label, kind, baseline):
        return {'label': label, 'repository': args.repository, 'trusted_keys': keys,
                'target': kind, 'baseline': str(baseline.resolve())}
    targets = {'application': target('Paperphase', {'kind': 'application', 'platform': args.platform}, args.application)}
    if args.bindings:
        bindings = json.loads(args.bindings.read_text())
        for binding in bindings['bindings']:
            connector = binding['connector_id']
            module = Path(binding['wasm'])
            if not module.is_absolute():
                module = args.working_dir / module
            if not module.is_file():
                parser.error(f'Installed module is missing for {connector}')
            targets[connector] = target(LABELS.get(connector, connector),
                                        {'kind': 'plugin', 'connector_id': connector, 'abi': 'paperphase_v0'}, module)
    value = {'schema_version': 1, 'root': str(root), 'data_dir': str(data),
             'working_dir': str(args.working_dir.resolve()), 'health_url': args.health_url,
             'status_port': args.status_port, 'app_args': arguments, 'targets': targets}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    descriptor = os.open(args.output, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, 'w') as output:
        json.dump(value, output, indent=2)
        output.write('\n')
        output.flush()
        os.fsync(output.fileno())
    print(f'Created supervisor configuration for {len(targets)} targets. Existing state and host grants were not changed.')


if __name__ == '__main__':
    main()
