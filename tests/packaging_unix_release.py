"""Unix payload packaging tests. Native app/helper inputs here are layout fixtures only."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tarfile
import tempfile
import unittest
import zipfile
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
CACHE = Path(os.environ.get('TERMCRAFT_TEST_UNIX_CACHE', ROOT / 'target/p05-cache'))
IPERF_ROOT = Path(os.environ.get('TERMCRAFT_TEST_IPERF_ROOT', ROOT))

def module(path):
    spec = importlib.util.spec_from_file_location('package_release', path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result

@unittest.skipUnless(sys.platform.startswith("linux") and shutil.which("cc") and CACHE.is_dir(),
                     "Requires Linux, cc, and the pinned offline Unix cache")
class UnixRelease(unittest.TestCase):
    def setUp(self):
        self.package = module(ROOT / 'scripts/package-release.py')
        self.temp = tempfile.TemporaryDirectory(prefix='unix payload layout ')
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.app, self.helper = self.base / 'app', self.base / 'helper'
        for path in (self.app, self.helper):
            subprocess.run(['cc', '-xc', '-o', path, '-'], input='int main(){return 0;}', text=True, check=True)

    def release(self, **kwargs):
        with patch.object(self.package, 'cargo_notices', return_value=[]):
            return self.package.package_release('linux', self.base / 'out', self.app, self.helper,
                                                source=kwargs.pop('source', ROOT), **kwargs)

    def unpack(self, path):
        destination = self.base / 'unpack'
        module(ROOT / 'scripts/portable_archive.py').extract_tar(path, destination)
        root = next(destination.iterdir())
        return root, json.loads((root / 'PACKAGE-MANIFEST.json').read_text())

    def test_optional_payload_inventory_sources_notices_and_no_overwrite(self):
        before = {p.name: self.package.sha256(p) for p in CACHE.iterdir()}
        archive = self.release(unix_cache=CACHE)
        self.assertTrue(archive.name.endswith('-unix.tar.gz'))
        root, manifest = self.unpack(archive)
        inventory = manifest['runtime']['portable_unix']
        self.assertEqual(inventory['platform'], 'linux')
        self.assertEqual(inventory['architecture'], 'x86_64')
        self.assertEqual(len(inventory['releases']), 2)
        old = next(r for r in inventory['releases'] if r['version'] == '1.31.0')
        self.assertEqual(len(old['build_information']), 2)
        for material in old['build_information']:
            self.assertEqual(self.package.sha256(root/material['path']), material['sha256'])
        for release in inventory['releases']:
            self.assertEqual(self.package.sha256(root / release['archive_path']), release['sha256'])
            self.assertEqual(self.package.sha256(root / release['source_path']), release['source_sha256'])
            self.assertTrue(release['notice_paths'])
            self.assertIn('musl', release['linked_components'][0]['name'])
            for notice in release['notice_paths']:
                self.assertTrue((root / notice).is_file())
            with tarfile.open(root / release['archive_path']) as payload:
                command = payload.extractfile('bin/ash').read()
            self.assertEqual(hashlib.sha256(command).hexdigest(), release['binary_sha256'])
            # Execute only the already hash-verified Linux shell, without host tools.
            smoke = self.base / ('linux-smoke-' + release['version'])
            smoke.mkdir()
            shell = smoke / 'ash'
            shell.write_bytes(command)
            shell.chmod(0o700)
            fixture = smoke / 'input'
            fixture.write_text('portable shell data\n')
            result = subprocess.run([shell, '-c', 'read -r line < input; printf "%s\\n" "$line"'],
                                    cwd=smoke, env={'PATH':'', 'HOME':str(smoke)},
                                    capture_output=True, text=True, check=True)
            self.assertEqual(result.stdout, 'portable shell data\n')
        config = next(r for r in inventory['releases'] if r['version'] == '1.35.0')
        self.assertEqual(self.package.sha256(root / config['config_path']), config['config_sha256'])
        self.assertEqual(len(list((root / 'assets').iterdir())), 6)
        self.assertEqual(set(manifest['files']), {p.relative_to(root).as_posix() for p in root.rglob('*') if p.is_file()} - {'PACKAGE-MANIFEST.json'})
        digest = self.package.sha256(archive)
        with self.assertRaises(FileExistsError): self.release(unix_cache=CACHE)
        self.assertEqual(self.package.sha256(archive), digest)
        self.assertEqual(before, {p.name: self.package.sha256(p) for p in CACHE.iterdir()})

    def test_absent_option_has_no_payload(self):
        root, manifest = self.unpack(self.release())
        self.assertNotIn('portable_unix', manifest['runtime'])
        self.assertFalse((root / 'unix-packages').exists())

    def test_corrupt_binary_source_and_missing_cache_fail_closed(self):
        index = json.loads((ROOT / 'packaging/unix-runtime/manifest.json').read_text())
        for key in ('binary_sha256', 'source_sha256', 'config_sha256'):
            with self.subTest(key=key):
                cache = self.base / key
                shutil.copytree(CACHE, cache)
                release = next(r for r in index['releases'] if r['platform'] == 'linux' and key in r)
                (cache / release[key]).write_bytes(b'tampered')
                with self.assertRaises((OSError, ValueError, subprocess.CalledProcessError)):
                    self.release(unix_cache=cache)
        with self.assertRaises((OSError, ValueError, subprocess.CalledProcessError)):
            self.release(unix_cache=self.base / 'missing')
        self.assertEqual(list((self.base / 'out').glob('*.tar.gz')), [])

    def test_unsupported_target_and_architecture_rejected(self):
        for target, arch in [('macos','x86_64'), ('linux','aarch64'), ('windows','aarch64')]:
            with self.subTest(target=target, arch=arch), patch.object(self.package, 'binary_arch', return_value=arch), patch.object(self.package.platform, 'machine', return_value=arch):
                with self.assertRaisesRegex(ValueError, 'Unix.*x86_64'):
                    self.package.package_release(target, self.base/'out', self.app, self.helper, source=ROOT, unix_cache=CACHE)

    def test_windows_payload_verified_without_execution(self):
        stage = module(ROOT / 'packaging/unix-runtime/stage-release.py')
        destination = self.base / 'windows'
        with patch.object(subprocess, 'run', side_effect=AssertionError('must not execute payload')):
            inventory = stage.stage(ROOT / 'packaging/unix-runtime', destination, CACHE, 'windows', 'x86_64')
        self.assertEqual(len(inventory['releases']), 1)
        release = inventory['releases'][0]
        self.assertTrue((destination / release['archive_path']).is_file())
        self.assertTrue(any('GNOME' in (destination/p).read_text() for p in release['notice_paths']))
        self.assertEqual(release['native_format'], 'PE-x86_64')

    def test_windows_zip_layout_contains_native_payload_and_source_inventory(self):
        # Header-only PE app/helper fixtures check archive layout, never Windows startup.
        for path in (self.app, self.helper):
            data = bytearray(256)
            data[:2] = b'MZ'
            struct.pack_into('<I', data, 60, 128)
            data[128:132] = b'PE\0\0'
            struct.pack_into('<H', data, 132, 0x8664)
            path.write_bytes(data)
        with patch.object(self.package, 'cargo_notices', return_value=[]), patch.object(self.package, 'native_libraries', return_value={'fixture_only': True}):
            archive = self.package.package_release('windows', self.base/'out', self.app, self.helper,
                                                   source=ROOT, unix_cache=CACHE)
        self.assertTrue(archive.name.endswith('-windows-x86_64-unix.zip'))
        with zipfile.ZipFile(archive) as zipped:
            zipped.extractall(self.base/'zip')
        root = next((self.base/'zip').iterdir())
        manifest = json.loads((root/'PACKAGE-MANIFEST.json').read_text())
        self.assertEqual(manifest['profile'], 'unverified')
        self.assertTrue((root/'harbor-workspace.exe').is_file())
        self.assertTrue((root/'termcraft-rdp-engine.exe').is_file())
        payload = manifest['runtime']['portable_unix']['releases'][0]
        self.assertTrue((root/payload['archive_path']).is_file())
        self.assertTrue((root/payload['source_path']).is_file())
        self.assertEqual(payload['native_format'], 'PE-x86_64')
        self.assertEqual(set(manifest['files']), {p.relative_to(root).as_posix() for p in root.rglob('*') if p.is_file()} - {'PACKAGE-MANIFEST.json'})
        for relative, expected in manifest['files'].items():
            self.assertEqual(self.package.sha256(root/relative), expected)

    def test_missing_or_symlink_input_never_downloads(self):
        stage = module(ROOT / 'packaging/unix-runtime/stage-release.py')
        import urllib.request
        cache = self.base / 'cache'
        shutil.copytree(CACHE, cache)
        pin = json.loads((ROOT/'packaging/unix-runtime/manifest.json').read_text())['releases'][0]['binary_sha256']
        (cache/pin).unlink()
        with patch.object(urllib.request, 'urlopen', side_effect=AssertionError('network forbidden')):
            with self.assertRaises(ValueError):
                stage.stage(ROOT/'packaging/unix-runtime', self.base/'missing staged', cache, 'linux', 'x86_64')
            (cache/pin).symlink_to(CACHE/pin)
            with self.assertRaisesRegex(ValueError, 'regular Unix'):
                stage.stage(ROOT/'packaging/unix-runtime', self.base/'symlink staged', cache, 'linux', 'x86_64')

    def test_archive_digest_native_format_and_notice_tampering_rejected(self):
        stage = module(ROOT / 'packaging/unix-runtime/stage-release.py')
        for change in ('archive', 'native', 'notice'):
            with self.subTest(change=change):
                here = self.base / change
                shutil.copytree(ROOT / 'packaging/unix-runtime', here)
                index = json.loads((here / 'manifest.json').read_text())
                if change == 'archive':
                    index['releases'][0]['sha256'] = '0' * 64
                elif change == 'native':
                    # A Windows pin in a Linux declaration is still rejected by its format.
                    index['releases'][0]['binary_sha256'] = index['releases'][2]['binary_sha256']
                else:
                    (here/'notices/musl-v1.1.22-COPYRIGHT').write_text('tampered')
                (here / 'manifest.json').write_text(json.dumps(index))
                with self.assertRaisesRegex(ValueError, 'checksum|match linux'):
                    stage.stage(here, self.base / ('staged-'+change), CACHE, 'linux', 'x86_64')

    def test_frozen_source_snapshot_remains_independent_and_unchanged(self):
        source = self.base / 'mutable source'
        for relative in self.package.source_inputs(ROOT):
            dest = source / relative
            dest.parent.mkdir(parents=True,exist_ok=True)
            shutil.copyfile(ROOT / relative, dest)
        frozen, inputs = self.package.freeze_sources(source, self.base / 'snapshots')
        (source / 'packaging/unix-runtime/manifest.json').write_text('{"invalid":true}')
        archive = self.release(unix_cache=CACHE, source=frozen)
        root, manifest = self.unpack(archive)
        self.assertEqual(self.package.source_inputs(frozen), inputs)
        self.assertEqual(manifest['build']['source_inputs'], inputs)
        self.assertEqual(len(manifest['runtime']['portable_unix']['releases']), 2)
        self.assertEqual(len(list((root/'assets').iterdir())), 6)

    def test_cli_exposes_offline_option(self):
        result = subprocess.run([sys.executable, ROOT/'scripts/package-release.py', '--help'],
                                capture_output=True, text=True, check=True)
        self.assertIn('--unix-cache', result.stdout)

    def test_composes_with_xwayland_and_mosh(self):
        def xwayland(source, resources, archive):
            (resources/'fake-Xwayland').write_bytes(b'layout fixture')
            return {'entry_point':'fake-Xwayland'}, {'libraries':[]}
        def mosh(source, resources, inputs):
            (resources/'fake-mosh').write_bytes(b'layout fixture')
            return {'mosh_inputs':{'fixture':True}}, 'fake-mosh', {'libraries':[]}
        iperf = IPERF_ROOT if (IPERF_ROOT/'runtime/linux-x86_64/iperf2').is_file() else None
        with patch.object(self.package,'stage_xwayland',side_effect=xwayland), patch.object(self.package,'stage_mosh',side_effect=mosh):
            archive = self.release(unix_cache=CACHE, xwayland_archive=Path('fixture'), mosh_inputs=('fixture',)*3, iperf_runtime=iperf)
        self.assertTrue(archive.name.endswith('-arch-xwayland-mosh-unix.tar.gz'))
        root, manifest = self.unpack(archive)
        self.assertIn('portable_unix',manifest['runtime'])
        self.assertIn('managed_xwayland',manifest['runtime'])
        self.assertIn('mosh',manifest['runtime'])
        if iperf is not None:
            self.assertEqual(manifest['runtime']['managed_iperf'], 'runtime/linux-x86_64/IPERF-MANIFEST.json')
            for name in ('iperf2','iperf3'):
                self.assertTrue((root/'runtime/linux-x86_64'/name).is_file())
                self.assertIn('runtime/linux-x86_64/'+name, manifest['runtime']['linked_libraries'])
                self.assertTrue((root/'runtime/linux-x86_64'/(name+'-source.tar.gz')).is_file())
                self.assertTrue((root/'runtime/linux-x86_64'/(name+'-LICENSE')).is_file())

if __name__ == '__main__': unittest.main(verbosity=2)
