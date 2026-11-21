import base64
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / 'configure-updates.py'


class UpdateConfigurationTests(unittest.TestCase):
    def test_inventory_keeps_host_grants_and_private_state_unchanged(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            data = root / 'data'
            data.mkdir()
            state = data / 'workspace.json'
            state.write_text('{"private":"fixture-owner-data"}')
            module = root / 'plugin.wasm'
            module.write_bytes(b'fixture-module')
            app = root / 'paperphase'
            app.write_bytes(b'fixture-application')
            bindings = data / 'bindings.json'
            bindings.write_text(json.dumps({'schema_version': 1, 'bindings': [
                {'connector_id': 'rss.headlines', 'wasm': str(module),
                 'source_url': 'https://example.invalid/?token=fixture-private-token',
                 'stale_after_secs': 60}]}))
            before = (state.read_bytes(), bindings.read_bytes())
            output = root / 'installation.json'
            command = [sys.executable, str(SCRIPT), '--output', str(output),
                       '--root', str(root / 'managed'), '--data-dir', str(data),
                       '--application', str(app), '--platform', 'x86_64-unknown-linux-gnu', '--bindings', str(bindings),
                       '--publisher', 'test=' + base64.b64encode(b'x' * 32).decode(),
                       '--', 'serve', '--state-file', str(state)]
            result = subprocess.run(command, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            value = json.loads(output.read_text())
            self.assertEqual(set(value['targets']), {'application', 'rss.headlines'})
            self.assertNotIn('source_url', value['targets']['rss.headlines'])
            self.assertNotIn('fixture-private-token', output.read_text() + result.stdout)
            self.assertEqual(before, (state.read_bytes(), bindings.read_bytes()))
            self.assertEqual(output.stat().st_mode & 0o777, 0o600)
            second = subprocess.run(command, capture_output=True, text=True)
            self.assertNotEqual(second.returncode, 0)
            self.assertEqual(value, json.loads(output.read_text()))

    def test_installation_cannot_be_inside_its_data_backup(self):
        with tempfile.TemporaryDirectory() as directory:
            data = Path(directory)
            app = data / 'paperphase'
            app.write_bytes(b'fixture')
            output = data / 'configuration.json'
            result = subprocess.run([sys.executable, str(SCRIPT), '--output', str(output),
                                     '--root', str(data / 'managed'), '--data-dir', str(data),
                                     '--application', str(app), '--platform', 'x86_64-unknown-linux-gnu', '--publisher',
                                     'test=' + base64.b64encode(b'x' * 32).decode(),
                                     '--', 'serve'], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(b'separate folders', result.stderr)
            self.assertFalse(output.exists())


if __name__ == '__main__':
    unittest.main()
