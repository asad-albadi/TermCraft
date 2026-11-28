import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = Path('packaging/browser/runtime-manifest-windows.json')


def load(name):
    path = ROOT / 'scripts' / name
    if not path.exists():
        return None
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


class BrowserQualification(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.source = self.root / 'private-source'
        self.source.mkdir()
        manifest = self.source / MANIFEST
        manifest.parent.mkdir(parents=True)
        self.original = (ROOT / MANIFEST).read_bytes()
        manifest.write_bytes(self.original)
        for command in (['init', '-q'], ['add', '.'], ['-c', 'user.name=Fixture', '-c',
                         'user.email=fixture@example.invalid', '-c', 'commit.gpgsign=false',
                         'commit', '-qm', 'Private fixture']):
            subprocess.run(['git', '-C', str(self.source), *command], check=True, capture_output=True)
        self.intake = load('browser-intake.py')
        self.preparation = load('prepare-browser-qualification.py')
        self.candidate = json.loads((ROOT / 'packaging/browser/windows-public-intake.json').read_text())
        self.archive = self.root / 'candidate.zip'
        image = bytearray(256)
        image[:2] = b'MZ'
        image[60:64] = (128).to_bytes(4, 'little')
        image[128:134] = b'PE\0\0\x64\x86'
        with zipfile.ZipFile(self.archive, 'w') as archive:
            archive.writestr(self.candidate['archive_root'] + '/chrome.exe', image)
            archive.writestr(self.candidate['archive_root'] + '/resources.pak', b'fixture resources')
        self.candidate['archive_sha256'] = self.intake.digest_file(self.archive)
        self.candidate['archive_size'] = self.archive.stat().st_size
        self.candidate_file = self.root / 'candidate.json'
        self.candidate_file.write_text(json.dumps(self.candidate))
        self.evidence = self.root / 'evidence'
        self.evidence.mkdir()
        inventory = self.intake.inspect_archive(self.archive, self.candidate)
        (self.evidence / 'archive-inventory.json').write_text(json.dumps(inventory))
        notices = b'<html>' + b'Copyright fixture corresponding notice. ' * 600 + b'</html>'
        (self.evidence / 'THIRD-PARTY-NOTICES.html').write_bytes(notices)
        self.record = {'schema_version': 1, 'scope': 'qualification-only', 'approved': False,
                       'candidate': self.candidate, 'native_capture': True,
                       'redistribution_approved': False, 'temporary_runtime_removed': True,
                       'inventory_sha256': self.intake.digest_file(self.evidence / 'archive-inventory.json'),
                       'credits': self.intake.credits_summary(notices),
                       'process': {'exit_code': 0, 'timed_out': False, 'assigned_before_resume': True,
                                   'cleanup': {'active_processes': 0, 'job_terminated': True, 'kill_on_close': True}}}
        self.write_record()

    def write_record(self):
        (self.evidence / 'intake.json').write_text(json.dumps(self.record))

    def prepare(self, source=None):
        self.assertIsNotNone(self.preparation, 'Private qualification preparation is missing')
        return self.preparation.prepare(self.candidate_file, self.archive, self.evidence,
                                        source or self.source)

    def test_stages_exact_inventory_and_notices_only_in_private_source(self):
        report = self.prepare()
        bundle = self.source / 'target/debug/deps/browser/windows-x86_64'
        compiled = (self.source / MANIFEST).read_bytes()
        self.assertEqual(compiled, (bundle / 'BROWSER-MANIFEST.json').read_bytes())
        manifest = json.loads(compiled)
        self.assertIs(manifest['approved'], True)
        self.assertEqual(manifest['scope'], 'qualification-only')
        self.assertEqual({f['path'] for f in manifest['files']},
                         {'chrome.exe', 'resources.pak', 'THIRD-PARTY-NOTICES.html'})
        self.assertEqual((bundle / 'THIRD-PARTY-NOTICES.html').read_bytes(),
                         (self.evidence / 'THIRD-PARTY-NOTICES.html').read_bytes())
        self.assertEqual((ROOT / MANIFEST).read_bytes(), self.original)
        self.assertEqual(report['source_sha'], subprocess.check_output(
            ['git', '-C', str(self.source), 'rev-parse', 'HEAD'], text=True).strip())
        self.assertEqual(report['original_manifest_sha256'], hashlib.sha256(self.original).hexdigest())
        self.assertIn('"approved": true', report['source_diff'])
        self.assertIs(report['production_enrollment'], False)

    def test_actual_checkout_and_symlink_source_are_refused(self):
        with self.assertRaisesRegex(ValueError, 'original checkout'):
            self.prepare(ROOT)
        link = self.root / 'source-link'
        try:
            link.symlink_to(self.source, target_is_directory=True)
        except OSError:
            self.skipTest('Directory symlink permission unavailable')
        with self.assertRaisesRegex(ValueError, 'symlink|reparse'):
            self.prepare(link)
        self.assertEqual((ROOT / MANIFEST).read_bytes(), self.original)

    def test_failed_or_forged_native_records_are_refused_before_writes(self):
        for change in ('capture', 'timeout', 'exit', 'drain', 'inventory', 'candidate'):
            original = json.loads(json.dumps(self.record))
            if change == 'capture': self.record['native_capture'] = False
            if change == 'timeout': self.record['process']['timed_out'] = True
            if change == 'exit': self.record['process']['exit_code'] = 1
            if change == 'drain': self.record['process']['cleanup']['active_processes'] = 1
            if change == 'inventory': self.record['inventory_sha256'] = '0' * 64
            if change == 'candidate': self.record['candidate']['archive_sha256'] = '0' * 64
            self.write_record()
            with self.subTest(change=change), self.assertRaises(ValueError): self.prepare()
            self.assertEqual((self.source / MANIFEST).read_bytes(), self.original)
            self.record = original

    def test_changed_credits_archive_or_inventory_are_refused(self):
        for name in ('THIRD-PARTY-NOTICES.html', 'archive-inventory.json'):
            target = self.evidence / name
            original = target.read_bytes()
            target.write_bytes(original + b'changed')
            with self.subTest(name=name), self.assertRaises(ValueError): self.prepare()
            target.write_bytes(original)
        with self.archive.open('ab') as stream: stream.write(b'changed')
        with self.assertRaisesRegex(ValueError, 'checksum|size'): self.prepare()
        self.assertEqual((self.source / MANIFEST).read_bytes(), self.original)

    def test_existing_bundle_and_dirty_source_are_refused(self):
        bundle = self.source / 'target/debug/deps/browser/windows-x86_64'
        bundle.mkdir(parents=True)
        with self.assertRaisesRegex(ValueError, 'existing bundle'): self.prepare()
        shutil.rmtree(bundle)
        (self.source / MANIFEST).write_bytes(self.original + b' ')
        with self.assertRaisesRegex(ValueError, 'tracked changes'): self.prepare()

    def test_report_write_failure_rolls_back_private_pin_and_new_bundle(self):
        from unittest.mock import patch
        self.assertIsNotNone(self.preparation)
        with patch.object(self.preparation.json, 'dump', side_effect=OSError('report disk full')):
            with self.assertRaisesRegex(OSError, 'report disk full'):
                self.prepare()
        self.assertEqual((self.source / MANIFEST).read_bytes(), self.original)
        self.assertFalse((self.source / 'target/debug/deps/browser/windows-x86_64').exists())
        self.assertFalse((self.evidence / 'qualification-preparation.json').exists())
        self.assertEqual((ROOT / MANIFEST).read_bytes(), self.original)

    def test_concurrent_report_publication_is_not_overwritten_and_rolls_back(self):
        from unittest.mock import patch
        self.assertIsNotNone(self.preparation)
        report_path = self.evidence / 'qualification-preparation.json'

        def concurrent_report(*arguments, **keywords):
            report_path.write_bytes(b'other writer retained evidence')
            raise FileExistsError('concurrent report')

        with patch.object(self.preparation.os, 'link', side_effect=concurrent_report):
            with self.assertRaisesRegex(FileExistsError, 'concurrent report'):
                self.prepare()
        self.assertEqual(report_path.read_bytes(), b'other writer retained evidence')
        self.assertEqual((self.source / MANIFEST).read_bytes(), self.original)
        self.assertFalse((self.source / 'target/debug/deps/browser/windows-x86_64').exists())
        self.assertEqual((ROOT / MANIFEST).read_bytes(), self.original)
