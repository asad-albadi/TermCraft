#!/usr/bin/env python3
"""Actual signed-source staging regression; downloads/builds only in an owned checkout subtree."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent

class FreshSignedStaging(unittest.TestCase):
    def test_unmanaged_destination_is_refused_without_changes(self):
        parent = ROOT / 'target/interop-tools'
        parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix='unmanaged-regression-', dir=parent) as temporary:
            destination = Path(temporary) / 'dropbear-2026.94'
            destination.mkdir()
            sentinel = destination / 'user-owned'
            sentinel.write_bytes(b'PRESERVE_UNMANAGED')
            result = subprocess.run([str(ROOT / 'scripts/prepare-dropbear-interop.sh')], env=dict(os.environ, TERMCRAFT_DROPBEAR_DIR=str(destination)), capture_output=True, text=True, timeout=10)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('Refusing to replace unmanaged', result.stderr)
            self.assertEqual(sentinel.read_bytes(), b'PRESERVE_UNMANAGED')
            self.assertFalse(destination.is_symlink())
            print('ASSERTED: unmanaged existing destination refused and preserved')

    def test_previous_source_and_object_tree_cannot_execute_or_influence_build(self):
        parent = ROOT / 'target/interop-tools'
        parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix='staging-regression-', dir=parent) as temporary:
            owned = Path(temporary)
            previous = owned / '.stages/old/dropbear-2026.94'
            previous.mkdir(parents=True)
            marker = owned / 'tampered-executed'
            configure = previous / 'configure'
            configure.write_text('#!/bin/sh\ntouch "' + str(marker) + '"\nexit 42\n')
            configure.chmod(0o700)
            (previous / 'obj').mkdir()
            (previous / 'obj/svr-main.o').write_bytes(b'TAMPERED_OBJECT_MUST_NOT_BE_REUSED')
            (previous / 'src').mkdir()
            (previous / 'src/svr-main.c').write_text('#error TAMPERED_SOURCE_MUST_NOT_BE_BUILT\n')
            destination = owned / 'dropbear-2026.94'
            destination.symlink_to(previous, target_is_directory=True)
            (owned / '.termcraft-dropbear-owner').write_text('TermCraft private Dropbear fixture v1\n')
            result = subprocess.run([str(ROOT / 'scripts/prepare-dropbear-interop.sh')], env=dict(os.environ, TERMCRAFT_DROPBEAR_DIR=str(destination)), capture_output=True, text=True, timeout=180)
            print(result.stdout + result.stderr)
            self.assertFalse(marker.exists(), 'preexisting unverified configure was executed')
            self.assertEqual(result.returncode, 0, 'fresh signed archive must build successfully')
            published = destination.resolve()
            self.assertNotEqual(published, previous)
            self.assertIn('/.stages/', str(published))
            self.assertNotIn('TAMPERED_SOURCE', (published / 'src/svr-main.c').read_text())
            self.assertEqual((previous / 'obj/svr-main.o').read_bytes(), b'TAMPERED_OBJECT_MUST_NOT_BE_REUSED')
            self.assertTrue((published / 'dropbear').is_file())
            print('ASSERTED: old configure never executed; old source/object untouched; fresh signed source published atomically')

if __name__ == '__main__': unittest.main()
