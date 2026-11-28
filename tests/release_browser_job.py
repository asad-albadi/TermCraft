import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


def module():
    path = ROOT / 'scripts/windows-browser-intake.py'
    if not path.exists():
        return None
    spec = importlib.util.spec_from_file_location('windows_browser_intake', path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


class BrowserJob(unittest.TestCase):
    def test_fixed_command_preserves_paths_and_security_defaults(self):
        implementation = module()
        self.assertIsNotNone(implementation, 'Owned Windows capture implementation is missing')
        image = Path('C:/candidate path/chrome.exe')
        profile = Path('C:/private path/profile')
        self.assertEqual(implementation.command(image, profile), [
            str(image), '--headless', '--allow-chrome-scheme-url', '--dump-dom',
            '--no-first-run', '--no-default-browser-check',
            '--disable-background-networking', '--disable-component-update',
            '--timeout=20000', '--user-data-dir=' + str(profile), 'chrome://credits/',
        ])

    def test_private_profile_creation_is_an_explicit_boundary(self):
        implementation = module()
        self.assertTrue(callable(getattr(implementation, '_private_profile', None)))
        if os.name != 'nt':
            with self.assertRaisesRegex(OSError, 'Windows'):
                implementation._private_profile(Path('profile'))

    def test_non_windows_refuses_execution(self):
        if os.name == 'nt':
            self.skipTest('Non-Windows refusal')
        implementation = module()
        self.assertIsNotNone(implementation)
        with self.assertRaisesRegex(OSError, 'Windows'):
            implementation.capture(Path('chrome.exe'), Path('profile'), Path('out'), Path('err'))

    @unittest.skipUnless(os.name == 'nt', 'Native Windows profile ACL fixture')
    def test_new_profile_has_protected_single_user_acl_and_cannot_be_reused(self):
        import ctypes as c
        from ctypes import wintypes as w
        implementation = module()
        with tempfile.TemporaryDirectory() as temporary:
            profile = Path(temporary) / 'profile'
            implementation._private_profile(profile)
            (profile / 'owner-readable').write_text('private')
            with self.assertRaises(FileExistsError):
                implementation._private_profile(profile)
            api = c.WinDLL('advapi32', use_last_error=True)
            get = api.GetNamedSecurityInfoW
            get.argtypes = [w.LPWSTR, c.c_int, w.DWORD, c.c_void_p, c.c_void_p,
                            c.c_void_p, c.c_void_p, c.POINTER(c.c_void_p)]
            get.restype = w.DWORD
            convert = api.ConvertSecurityDescriptorToStringSecurityDescriptorW
            convert.argtypes = [c.c_void_p, w.DWORD, w.DWORD, c.POINTER(w.LPWSTR), c.c_void_p]
            convert.restype = w.BOOL
            kernel = c.WinDLL('kernel32', use_last_error=True)
            kernel.LocalFree.argtypes = [c.c_void_p]
            kernel.LocalFree.restype = c.c_void_p
            descriptor, text = c.c_void_p(), w.LPWSTR()
            try:
                self.assertEqual(get(str(profile), 1, 4, None, None, None, None,
                                     c.byref(descriptor)), 0)
                self.assertTrue(convert(descriptor, 1, 4, c.byref(text), None))
                self.assertTrue(text.value.startswith('D:P'), text.value)
                self.assertEqual(text.value.count('(A;'), 1, text.value)
                self.assertIn(';FA;;;S-1-', text.value)
            finally:
                if text:
                    kernel.LocalFree(c.cast(text, c.c_void_p))
                if descriptor:
                    kernel.LocalFree(descriptor)

    @unittest.skipUnless(os.name == 'nt', 'Native Windows owned-process fixture')
    def test_timeout_reaps_owned_parent_and_descendant(self):
        implementation = module()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            # The descendant emits a marker on inherited stdout before sleeping.
            # The root child never exits by itself: the timeout must reap both.
            script = ('import subprocess,sys,time; '
                      'p=subprocess.Popen([sys.executable,"-c",'
                      '"import time; print(\'descendant ready\',flush=True); time.sleep(120)"]); '
                      'print("parent ready",flush=True); time.sleep(120)')
            result = implementation._run_owned([sys.executable, '-c', script],
                                               root / 'out', root / 'err', 4)
            self.assertTrue(result['timed_out'])
            self.assertTrue(result['assigned_before_resume'])
            self.assertEqual(result['cleanup']['active_processes'], 0)
            self.assertEqual(result['inherited_handle_count'], 3)
            self.assertIn('descendant ready', (root / 'out').read_text())
            self.assertIn('parent ready', (root / 'out').read_text())

    @unittest.skipUnless(os.name == 'nt', 'Native Windows handle inheritance fixture')
    def test_unrelated_inheritable_handle_is_excluded_from_child(self):
        import ctypes as c
        from ctypes import wintypes as w
        import msvcrt
        implementation = module()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            sentinel = root / 'unrelated-private-sentinel'
            sentinel.write_bytes(b'owned inheritance probe')
            fd = os.open(sentinel, os.O_RDONLY | os.O_BINARY)
            original = os.get_inheritable(fd)
            try:
                os.set_inheritable(fd, True)
                handle = msvcrt.get_osfhandle(fd)
                kernel = c.WinDLL('kernel32', use_last_error=True)
                get_path = kernel.GetFinalPathNameByHandleW
                get_path.argtypes = [w.HANDLE, w.LPWSTR, w.DWORD, w.DWORD]
                get_path.restype = w.DWORD
                path = c.create_unicode_buffer(32768)
                count = get_path(handle, path, len(path), 0)
                # Positive control: the very same handle resolves in its owner.
                self.assertGreater(count, 0)
                self.assertLess(count, len(path))
                owned_path = os.path.normcase(path.value)
                script = """import ctypes as c, json, sys
from ctypes import wintypes as w
kernel = c.WinDLL('kernel32', use_last_error=True)
get_path = kernel.GetFinalPathNameByHandleW
get_path.argtypes = [w.HANDLE, w.LPWSTR, w.DWORD, w.DWORD]
get_path.restype = w.DWORD
path = c.create_unicode_buffer(32768)
count = get_path(int(sys.argv[1]), path, len(path), 0)
print(json.dumps({'stdout': 'owned output works',
                  'resolved': path.value if 0 < count < len(path) else None}), flush=True)
"""
                result = implementation._run_owned([sys.executable, '-c', script, str(handle)],
                                                   root / 'out', root / 'err', 10)
                self.assertFalse(result['timed_out'])
                self.assertEqual(result['exit_code'], 0, (root / 'err').read_text())
                observed = json.loads((root / 'out').read_text())
                self.assertEqual(observed['stdout'], 'owned output works')
                # The handle may be invalid or reused by Python, but must never
                # name the owner's unrelated sentinel file in this child.
                resolved = observed['resolved']
                self.assertTrue(resolved is None or os.path.normcase(resolved) != owned_path,
                                'Unrelated inheritable file handle leaked into the child')
                self.assertEqual(result['cleanup']['active_processes'], 0)
            finally:
                os.set_inheritable(fd, original)
                os.close(fd)

    @unittest.skipUnless(os.name == 'nt', 'Native Windows owned-process fixture')
    def test_success_records_exact_exit_and_refuses_output_replacement(self):
        implementation = module()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            result = implementation._run_owned([sys.executable, '-c', 'print("owned child")'],
                                               root / 'out', root / 'err', 10)
            self.assertFalse(result['timed_out'])
            self.assertEqual(result['exit_code'], 0)
            self.assertEqual(result['cleanup']['active_processes'], 0)
            with self.assertRaises(FileExistsError):
                implementation._run_owned([sys.executable, '-c', 'print("replacement")'],
                                           root / 'out', root / 'err', 10)
            self.assertEqual((root / 'out').read_text().strip(), 'owned child')
