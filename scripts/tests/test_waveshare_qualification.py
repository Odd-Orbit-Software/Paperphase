"""Qualification frame isolation and durable cooldown tests; no hardware."""
import base64
import importlib.util
import json
from pathlib import Path
import stat
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('qualification', Path(__file__).resolve().parents[1] / 'waveshare-v2-qualify.py')
qualification = importlib.util.module_from_spec(spec)
spec.loader.exec_module(qualification)


def journal():
    return {'schema_version': 1, 'not_before': 123,
            'completed': {'revision': 7, 'completed_at': 100,
                          'bits': base64.b64encode(bytes([0x55]) * 48000).decode(),
                          'layout': {'display': {'width': 800, 'height': 480},
                                     'components': [{'rect': {'x': 0, 'y': 0, 'width': 800, 'height': 100}}]}}}


class QualificationTests(unittest.TestCase):
    def test_markers_preserve_all_pixels_outside_the_check_rectangle(self):
        record = journal()
        before, after = qualification.frames(record, (24, 157, 160, 48))
        self.assertEqual(len(before), 48011)
        self.assertNotEqual(before, after)
        for row in range(480):
            start, end = row * 100 + 11, (row + 1) * 100 + 11
            if 157 <= row < 205:
                for image in (before, after):
                    self.assertEqual(image[start:start + 3], bytes([0x55]) * 3)
                    self.assertEqual(image[start + 23:end], bytes([0x55]) * 77)
            else:
                self.assertEqual(before[start:end], bytes([0x55]) * 100)
                self.assertEqual(after[start:end], bytes([0x55]) * 100)
        self.assertEqual(record, journal())

    def test_overlaps_bounds_and_alignment_rejected(self):
        for region in [(24, 99, 160, 48), (25, 157, 160, 48), (24, 450, 160, 48), (24, 157, 159, 48)]:
            with self.subTest(region=region), self.assertRaises(ValueError):
                qualification.frames(journal(), region)

    def test_atomic_cooldown_write_retains_receipt_and_private_permissions(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'workspace.display.json'
            record = journal()
            qualification.persist(path, record)
            record['not_before'] = 999
            qualification.persist(path, record)
            self.assertEqual(json.loads(path.read_text())['completed'], journal()['completed'])
            self.assertEqual(json.loads(path.read_text())['not_before'], 999)
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)
            self.assertEqual(list(path.parent.iterdir()), [path])


if __name__ == '__main__':
    unittest.main()
