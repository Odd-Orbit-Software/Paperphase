"""Electrical-adapter contract tests; fake vendor never imports real GPIO."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

WORKER = Path(__file__).resolve().parents[2] / 'crates/paperphase/src/display/waveshare_v2.py'
CONFIG = '''from pathlib import Path
import time
RST_PIN,DC_PIN,BUSY_PIN,PWR_PIN=17,25,24,18
root=Path(__file__).parent.parent
(root/'imported').write_text('yes')
reads=0
def digital_read(pin):
 global reads
 reads+=1
 return reads%2==0
def module_exit(cleanup=False):
 (root/'released').write_text(str(cleanup))
'''
PANEL = '''from . import epdconfig
class EPD:
 width,height,busy_pin=800,480,24
 def send_command(self,command): pass
 def send_data2(self,data):
  assert isinstance(data,bytes) and len(data)==48000
 def init(self):self.ReadBusy();return 0
 def display(self,data):
  assert len(data)==48000
  self.send_data2([~value for value in data]);self.send_data2(data)
  self.ReadBusy()
  (epdconfig.root/'displayed').write_text('yes')
 def sleep(self):self.ReadBusy();epdconfig.module_exit()
'''

class AdapterTests(unittest.TestCase):
    def run_adapter(self,config=CONFIG,parent=None,valid=True):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        root=Path(self.temp.name);package=root/'waveshare_epd';package.mkdir()
        (package/'__init__.py').write_text('');(package/'epdconfig.py').write_text(config);(package/'epd7in5_V2.py').write_text(PANEL)
        frame=root/'frame.pbm';frame.write_bytes(b'P4\n800 480\n'+bytes([0x80])*48000 if valid else b'P4\nwrong\n')
        result=subprocess.run([sys.executable,'-I',str(WORKER),'--vendor-dir',str(root),'--pbm',str(frame),'--parent',str(parent if parent is not None else os.getpid())],capture_output=True,timeout=5)
        return root,result,json.loads(result.stdout)
    def test_full_frame_planes_busy_ack_sleep_and_cleanup(self):
        root,result,report=self.run_adapter();self.assertEqual(result.returncode,0,result.stderr);self.assertTrue(report['ok']);self.assertTrue(report['sleep_completed']);self.assertTrue(report['power_released']);self.assertEqual(len(report['busy_waits']),3);self.assertTrue(all(item['busy_observed'] for item in report['busy_waits']));self.assertTrue((root/'displayed').exists());self.assertEqual((root/'released').read_text(),'True')
    def test_bad_frame_does_not_import_gpio(self):
        root,result,report=self.run_adapter(valid=False);self.assertNotEqual(result.returncode,0);self.assertFalse(report['ok']);self.assertFalse((root/'imported').exists())
    def test_lost_parent_aborts_and_releases_power(self):
        root,result,report=self.run_adapter(parent=0);self.assertNotEqual(result.returncode,0);self.assertIn('owner exited',report['error']);self.assertTrue(report['power_released']);self.assertFalse((root/'displayed').exists())
    def test_busy_deadline_releases_power(self):
        config=CONFIG+'''\nclock=0\ndef monotonic():\n global clock\n clock+=21\n return clock\ntime.monotonic=monotonic\ndef digital_read(pin):return 0\n'''
        root,result,report=self.run_adapter(config=config);self.assertNotEqual(result.returncode,0);self.assertIn('BUSY exceeded',report['error']);self.assertTrue(report['power_released']);self.assertFalse((root/'displayed').exists())

if __name__=='__main__':unittest.main()
