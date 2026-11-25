"""Portable TAR extraction security and Python 3.11 compatibility."""
import importlib.util
import io
import json
import hashlib
import os
from pathlib import Path
import stat
import tarfile
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), ROOT / 'scripts' / (name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class PortableArchive(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.archive = self.root / 'fixture.tar.gz'
        self.debian = load('package-debian')
        self.startup = load('package-check-startup')

    def write(self, entries):
        with tarfile.open(self.archive, 'w:gz') as bundle:
            for name, kind, data, mode in entries:
                member = tarfile.TarInfo(name)
                member.type = kind
                member.mode = mode
                member.linkname = '../../outside'
                member.size = len(data) if kind == tarfile.REGTYPE else 0
                bundle.addfile(member, io.BytesIO(data) if member.size else None)

    def calls(self):
        return (self.debian.extract_portable, self.startup.extract)

    def link_directory(self, target, link):
        if os.name == "nt":
            import _winapi
            _winapi.CreateJunction(str(target), str(link))
        else:
            link.symlink_to(target, target_is_directory=True)

    def test_valid_payload_and_special_modes(self):
        files = {'harbor-workspace': b'app', 'termcraft-rdp-engine': b'helper'}
        manifest = {'platform': 'linux', 'profile': 'release', 'files': {
            name: hashlib.sha256(data).hexdigest() for name, data in files.items()}}
        files['PACKAGE-MANIFEST.json'] = json.dumps(manifest).encode()
        self.write([('TermCraft', tarfile.DIRTYPE, b'', 0o7777)] + [
            ('TermCraft/' + name, tarfile.REGTYPE, data, 0o7777) for name, data in files.items()])
        for i, call in enumerate(self.calls()):
            out = self.root / str(i)
            out.mkdir()
            call(self.archive, out)
            self.assertEqual((out / 'TermCraft/harbor-workspace').read_bytes(), b'app')
            for item in out.rglob('*'):
                if os.name == "nt":
                    self.assertTrue(item.stat().st_mode & stat.S_IWRITE)
                else:
                    self.assertEqual(stat.S_IMODE(item.stat().st_mode), 0o755)

    def test_reject_paths_duplicates_links_and_special_types_without_partial_output(self):
        regular = ('TermCraft/ok', tarfile.REGTYPE, b'ok', 0o644)
        bad = [
            [('TermCraft/../escape', tarfile.REGTYPE, b'x', 0o644)],
            [('/absolute', tarfile.REGTYPE, b'x', 0o644)],
            [('TermCraft//alias', tarfile.REGTYPE, b'x', 0o644)],
            [('TermCraft/./alias', tarfile.REGTYPE, b'x', 0o644)],
            [('TermCraft\\escape', tarfile.REGTYPE, b'x', 0o644)],
            [('C:escape', tarfile.REGTYPE, b'x', 0o644)],
            [regular],
            [('TermCraft', tarfile.REGTYPE, b'x', 0o644)],
        ]
        bad += [[('TermCraft/evil', kind, b'', 0o644)] for kind in (
            tarfile.SYMTYPE, tarfile.LNKTYPE, tarfile.CHRTYPE, tarfile.BLKTYPE,
            tarfile.FIFOTYPE, tarfile.CONTTYPE, tarfile.GNUTYPE_SPARSE)]
        bad += [[('TermCraft/ok', tarfile.DIRTYPE, b'', 0o755)]]
        for index, entries in enumerate(bad):
            self.write([regular] + entries)
            for i, call in enumerate(self.calls()):
                with self.subTest(case=index, consumer=i):
                    out = self.root / f'bad-{index}-{i}'
                    out.mkdir()
                    with self.assertRaises((ValueError, tarfile.TarError)):
                        call(self.archive, out)
                    self.assertEqual(list(out.iterdir()), [])
        self.assertFalse((self.root / 'outside').exists())
        self.assertFalse(any(p.name.startswith('.portable-extract-') for p in self.root.iterdir()))

    def test_symlink_destination_and_parent_rejected(self):
        self.write([('TermCraft/file', tarfile.REGTYPE, b'x', 0o644)])
        outside = self.root / 'outside'
        outside.mkdir()
        link = self.root / 'link'
        self.link_directory(outside, link)
        for call in self.calls():
            for out in (link, link / 'child'):
                with self.assertRaises(ValueError):
                    call(self.archive, out)
                self.assertEqual(list(outside.iterdir()), [])

    def test_resolved_trusted_temp_root_accepts_platform_alias(self):
        self.write([('TermCraft/file', tarfile.REGTYPE, b'x', 0o644)])
        real = self.root / 'real-temp-root'
        real.mkdir()
        alias = self.root / 'platform-temp-alias'
        self.link_directory(real, alias)
        # smoke() resolves its own freshly created trusted root before passing
        # the destination; arbitrary unresolved symlink paths remain rejected.
        self.startup.extract(self.archive, alias.resolve() / 'package files')
        self.assertEqual((real / 'package files/TermCraft/file').read_bytes(), b'x')

    def test_write_failure_and_nonempty_destination_are_preserved(self):
        self.write([('TermCraft/file', tarfile.REGTYPE, b'payload', 0o644)])
        for index, call in enumerate(self.calls()):
            out = self.root / ('write-failure-' + str(index))
            out.mkdir()
            with patch('shutil.copyfileobj', side_effect=OSError('injected disk write failure')):
                with self.assertRaises(OSError):
                    call(self.archive, out)
            self.assertEqual(list(out.iterdir()), [])
            marker = out / 'existing'
            marker.write_bytes(b'preserve')
            with self.assertRaises(ValueError):
                call(self.archive, out)
            self.assertEqual(marker.read_bytes(), b'preserve')
        self.assertFalse(any(p.name.startswith('.portable-extract-') for p in self.root.iterdir()))

    def test_frozen_scripts_can_import_the_frozen_extractor(self):
        package = load('package-release')
        frozen, inputs = package.freeze_sources(ROOT, self.root / 'snapshots')
        for relative in ('scripts/portable_archive.py', 'scripts/package-check-startup.py'):
            self.assertIn(relative, inputs)
            self.assertEqual(hashlib.sha256((frozen / relative).read_bytes()).hexdigest(), inputs[relative])
        self.write([('TermCraft/file', tarfile.REGTYPE, b'x', 0o644)])
        for index, name in enumerate(('package-debian', 'package-check-startup')):
            spec = importlib.util.spec_from_file_location('frozen_' + str(index), frozen / 'scripts' / (name + '.py'))
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            self.assertEqual(module.extract_tar.__code__.co_filename, str(frozen / 'scripts/portable_archive.py'))
            if index == 1:
                module.extract(self.archive, self.root / 'frozen-output')
                self.assertEqual((self.root / 'frozen-output/TermCraft/file').read_bytes(), b'x')

    def test_read_failure_leaves_no_partial_output(self):
        self.write([('TermCraft/file', tarfile.REGTYPE, b'x' * 10000, 0o644)])
        # A decompression error after headers have been seen must also clean up.
        self.archive.write_bytes(self.archive.read_bytes()[:40])
        for index, call in enumerate(self.calls()):
            out = self.root / ('truncated-' + str(index))
            out.mkdir()
            with self.assertRaises((EOFError, OSError, tarfile.TarError)):
                call(self.archive, out)
            self.assertEqual(list(out.iterdir()), [])
        self.assertFalse(any(p.name.startswith('.portable-extract-') for p in self.root.iterdir()))

    def test_manifest_tampering_and_missing_manifest_rejected(self):
        for files in ({'files': {}}, {'files': {'harbor-workspace': '0' * 64}}):
            manifest = {'platform': 'linux', 'profile': 'release', **files}
            self.write([('TermCraft/harbor-workspace', tarfile.REGTYPE, b'app', 0o755),
                        ('TermCraft/PACKAGE-MANIFEST.json', tarfile.REGTYPE, json.dumps(manifest).encode(), 0o644)])
            out = self.root / ('manifest-' + str(len(files['files'])))
            with self.assertRaises(ValueError):
                self.debian.extract_portable(self.archive, out)
            with self.assertRaises(ValueError):
                self.startup.verify(out / 'TermCraft', out / 'TermCraft/PACKAGE-MANIFEST.json')


if __name__ == '__main__':
    unittest.main()
