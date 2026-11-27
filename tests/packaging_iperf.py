"""The pinned engines, corresponding source and license must travel together."""
import importlib.util
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import json
import re
import socket
import time
from unittest.mock import patch
import tempfile
import unittest
ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('stage_iperf', ROOT/'packaging/iperf/stage-runtime.py')
stage = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stage)
class IperfStaging(unittest.TestCase):
    def test_qualified_payload_stages_and_rejects_tampering(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            payload = root/'payload'
            shutil.copytree(ROOT/'runtime/linux-x86_64', payload/'runtime/linux-x86_64')
            stage.stage(ROOT/'packaging/iperf/runtime-manifest.json', payload, root/'output')
            self.assertTrue((root/'output/runtime/linux-x86_64/iperf3').is_file())
            self.assertTrue((root/'output/runtime/linux-x86_64/iperf2-source.tar.gz').is_file())
            with self.assertRaises(FileExistsError):
                stage.stage(ROOT/'packaging/iperf/runtime-manifest.json', payload, root/'output')
            target = payload/'runtime/linux-x86_64/iperf3'
            target.write_bytes(b'replaced binary')
            with self.assertRaisesRegex(ValueError, 'checksum mismatch'):
                stage.stage(ROOT/'packaging/iperf/runtime-manifest.json', payload, root/'tampered')
    @unittest.skipUnless(sys.platform.startswith('linux') and shutil.which('readelf'), 'ELF ABI inspection')
    def test_engines_keep_the_debian12_glibc_baseline(self):
        for tool in ('iperf2', 'iperf3'):
            with self.subTest(tool=tool):
                binary = ROOT/'runtime/linux-x86_64'/tool
                symbols = subprocess.check_output(['readelf', '--version-info', str(binary)], text=True)
                required = {tuple(map(int, value.split('.'))) for value in re.findall(r'Name: GLIBC_([0-9.]+)', symbols)}
                self.assertTrue(required, 'expected versioned glibc imports')
                self.assertLessEqual(max(required), (2, 36), f'{tool} exceeds the Debian 12 glibc baseline')
                dynamic = subprocess.check_output(['readelf', '-d', str(binary)], text=True)
                self.assertNotRegex(dynamic, r'\((?:RPATH|RUNPATH)\)')

    @unittest.skipUnless(sys.platform.startswith('linux'), 'Linux native endpoints')
    def test_staged_engines_transfer_tcp_and_udp_on_loopback(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)/'output'
            stage.stage(ROOT/'packaging/iperf/runtime-manifest.json', ROOT, output)
            for tool in ('iperf2', 'iperf3'):
                binary = output/'runtime/linux-x86_64'/tool
                for udp in (False, True):
                    with self.subTest(tool=tool, udp=udp):
                        with socket.socket() as reservation:
                            reservation.bind(('127.0.0.1', 0))
                            port = str(reservation.getsockname()[1])
                        server_args = [str(binary), '-s', '-B', '127.0.0.1', '-p', port]
                        if tool == 'iperf2' and udp:
                            server_args.append('-u')
                        with tempfile.TemporaryFile() as log:
                            server = subprocess.Popen(server_args, stdout=log, stderr=log)
                            try:
                                time.sleep(0.2)
                                self.assertIsNone(server.poll(), 'staged server exited during startup')
                                client_args = [str(binary), '-c', '127.0.0.1', '-p', port, '-t', '1']
                                if udp:
                                    client_args += ['-u', '-b', '1M']
                                client_args += ['-y', 'C'] if tool == 'iperf2' else ['--json']
                                client = subprocess.run(client_args, capture_output=True, text=True, timeout=10)
                                self.assertEqual(client.returncode, 0, client.stderr + client.stdout)
                                if tool == 'iperf3':
                                    report = json.loads(client.stdout)
                                    self.assertNotIn('error', report)
                                    self.assertGreater(report['end']['sum_received']['bytes'], 0)
                                else:
                                    transfers = [line.split(',') for line in client.stdout.splitlines()]
                                    self.assertTrue(any(len(row) >= 9 and row[7].isdigit() and int(row[7]) > 0 for row in transfers), client.stdout)
                            finally:
                                server.terminate()
                                try:
                                    server.wait(timeout=3)
                                except subprocess.TimeoutExpired:
                                    server.kill()
                                    server.wait(timeout=3)

    def test_corresponding_source_and_license_are_verified(self):
        for suffix in ('-source.tar.gz','-LICENSE'):
            with self.subTest(suffix=suffix), tempfile.TemporaryDirectory() as temporary:
                root=Path(temporary)
                payload=root/'payload'
                shutil.copytree(ROOT/'runtime/linux-x86_64',payload/'runtime/linux-x86_64')
                (payload/('runtime/linux-x86_64/iperf2'+suffix)).write_bytes(b'modified')
                with self.assertRaisesRegex(ValueError,'checksum mismatch'):
                    stage.stage(ROOT/'packaging/iperf/runtime-manifest.json',payload,root/'output')
    @unittest.skipUnless(sys.platform.startswith('linux') and shutil.which('cc'), 'Linux native packaging test')
    def test_release_archive_includes_both_engines_and_declares_host_libraries(self):
        release_spec=importlib.util.spec_from_file_location('package_iperf_release', ROOT/'scripts/package-release.py')
        release=importlib.util.module_from_spec(release_spec)
        release_spec.loader.exec_module(release)
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            app=root/'harbor-workspace'
            helper=root/'termcraft-rdp-engine'
            for target in (app,helper):
                subprocess.run(['cc','-xc','-o',str(target),'-'],input='int main(){return 0;}\n',text=True,check=True)
            with patch.object(release,'cargo_notices',return_value=[]):
                archive=release.package_release('linux',root/'out',app,helper,iperf_runtime=ROOT)
            with tarfile.open(archive) as tar:
                names=tar.getnames()
                for name in ('iperf2','iperf3','iperf2-source.tar.gz','iperf3-LICENSE'):
                    self.assertTrue(any(p.endswith('/runtime/linux-x86_64/'+name) for p in names))
                member=next(p for p in names if p.endswith('/PACKAGE-MANIFEST.json'))
                manifest=json.load(tar.extractfile(member))
                self.assertEqual(manifest['runtime']['managed_iperf'],'runtime/linux-x86_64/IPERF-MANIFEST.json')
                self.assertIn('runtime/linux-x86_64/iperf2',manifest['runtime']['linked_libraries'])

if __name__ == '__main__': unittest.main()
