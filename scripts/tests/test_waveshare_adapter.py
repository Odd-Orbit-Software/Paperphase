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
  assert isinstance(data,bytes) and len(data)==getattr(self,'expected_bytes',48000)
  if len(data)!=48000:(epdconfig.root/'partial.bin').write_bytes(data)
 def init(self):self.ReadBusy();return 0
 def init_part(self):self.ReadBusy();return 0
 def display(self,data):
  assert len(data)==48000
  self.send_data2([~value for value in data]);self.send_data2(data)
  self.ReadBusy()
  (epdconfig.root/'displayed').write_text('yes')
 def display_Partial(self,data,x,y,end_x,end_y):
  assert x%8==0 and end_x%8==0 and x>0 and y>0
  self.expected_bytes=(end_x-x)*(end_y-y)//8
  padded=[255]*48000
  padded[:len(data)]=[~value for value in data]
  self.send_data2(padded)
  self.expected_bytes=48000
  self.ReadBusy()
 def sleep(self):self.ReadBusy();epdconfig.module_exit()
'''

class AdapterTests(unittest.TestCase):
    def run_adapter(self,config=CONFIG,parent=None,valid=True,partial=False,region=None,outside=False,panel=PANEL,wake=False):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        root=Path(self.temp.name);package=root/'waveshare_epd';package.mkdir()
        (package/'__init__.py').write_text('');(package/'epdconfig.py').write_text(config);(package/'epd7in5_V2.py').write_text(panel)
        frame=root/'frame.pbm';frame.write_bytes(b'P4\n800 480\n'+bytes([0x80])*48000 if valid else b'P4\nwrong\n')
        command=[sys.executable,'-I',str(WORKER),'--vendor-dir',str(root),'--pbm',str(frame),'--parent',str(parent if parent is not None else os.getpid())]
        if partial:
            target=bytearray([0x80]*48000);target[157*100+3]=0x7f;target[196*100+18]=0xff
            if outside:target[0]=0
            after=root/'after.pbm';after.write_bytes(b'P4\n800 480\n'+target)
            command+=['--partial-after',str(after),'--region',*map(str,region or [24,157,128,40])]
        if wake:command+=['--partial-only']
        result=subprocess.run(command,capture_output=True,timeout=5)
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
    def test_partial_crops_nonzero_origin_and_sends_only_addressed_bytes(self):
        root,result,report=self.run_adapter(partial=True)
        self.assertEqual(result.returncode,0,result.stdout);self.assertEqual(report['mode'],'full-then-partial-qualification')
        data=(root/'partial.bin').read_bytes();self.assertEqual(len(data),640)
        self.assertEqual(data[0],0x80);self.assertEqual(data[-1],0);self.assertEqual(data[1:-1],bytes([0x7f])*638)
        self.assertEqual(report['partial_payload_bytes'],640);self.assertTrue(report['sleep_completed']);self.assertTrue(report['power_released'])
        self.assertEqual(len(report['busy_waits']),5)
    def test_outside_change_and_unaligned_rectangle_rejected_before_gpio(self):
        for kwargs in [{'outside':True},{'region':[25,157,128,40]},{'region':[24,457,128,40]}]:
            with self.subTest(kwargs=kwargs):
                root,result,report=self.run_adapter(partial=True,**kwargs)
                self.assertNotEqual(result.returncode,0);self.assertFalse((root/'imported').exists())
    def test_partial_initialization_failure_releases_power(self):
        panel=PANEL.replace('def init_part(self):self.ReadBusy();return 0','def init_part(self):return -1')
        root,result,report=self.run_adapter(partial=True,panel=panel)
        self.assertNotEqual(result.returncode,0);self.assertIn('Partial initialization failed',report['error'])
        self.assertTrue(report['power_released']);self.assertFalse((root/'partial.bin').exists())
    def test_wake_partial_does_not_send_a_full_refresh(self):
        root,result,report=self.run_adapter(partial=True,wake=True)
        self.assertEqual(result.returncode,0,result.stdout);self.assertEqual(report['mode'],'partial')
        self.assertFalse((root/'displayed').exists());self.assertNotIn('refresh_ms',report)
        self.assertEqual(len((root/'partial.bin').read_bytes()),640);self.assertEqual(len(report['busy_waits']),3)
        self.assertTrue(report['sleep_completed']);self.assertTrue(report['power_released'])

if __name__=='__main__':unittest.main()
