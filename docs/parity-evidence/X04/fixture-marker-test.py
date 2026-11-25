"""Run the fixture's real publication AST with only its X11 values supplied.

The real write is paused after open/create and before writing, using events
rather than timing. This leaves the actual file operations and rename intact.
"""
import argparse
import ast
import io
import os
from pathlib import Path
import sys
import tempfile
import threading
from types import SimpleNamespace
from unittest.mock import patch


def publication_code(path):
    tree = ast.parse(path.read_text(), filename=str(path))
    nodes = []
    publication = []
    for node in tree.body:
        if isinstance(node, (ast.Import, ast.ImportFrom, ast.FunctionDef)):
            nodes.append(node)
        elif isinstance(node, ast.Expr):
            text = ast.get_source_segment(path.read_text(), node)
            if '.write_text(' in text or text.startswith('publish_marker('):
                publication.append(node)
    assert len(publication) == 1, 'identify exactly one actual fixture publication'
    return compile(ast.Module(body=nodes + publication, type_ignores=[]), str(path), 'exec')


def probe(fixture, expect_atomic, fail_write=False):
    code = publication_code(fixture / 'xdmcp_window.py')
    opened = threading.Event()
    resume = threading.Event()
    errors = []
    real_open = io.open
    with tempfile.TemporaryDirectory(prefix='xdmcp-publication-barrier-') as directory:
        marker = Path(directory) / 'ready'
        def blocked_open(*args, **kwargs):
            stream = real_open(*args, **kwargs)
            class BlockedWriter:
                def __enter__(self):
                    return self
                def __exit__(self, *exc):
                    return stream.__exit__(*exc)
                def write(self, value):
                    opened.set()
                    if not resume.wait(5):
                        raise TimeoutError('publication barrier was not released')
                    if fail_write:
                        raise OSError('injected write failure')
                    return stream.write(value)
                def __getattr__(self, key):
                    return getattr(stream, key)
            return BlockedWriter()
        def producer():
            try:
                exec(code, {
                    'sys': SimpleNamespace(argv=['xdmcp_window.py', str(marker)]),
                    'x11': SimpleNamespace(XDisplayWidth=lambda *_: 1280, XDisplayHeight=lambda *_: 720),
                    'display': None, 'screen': 0, 'window': 42,
                })
            except BaseException as error:
                errors.append(error)
        # Importing the fixture's sys overrides the namespace; supply argv and
        # DISPLAY at the same boundary the real xdm invocation supplies them.
        with patch.object(sys, 'argv', ['xdmcp_window.py', str(marker)]), patch.dict(os.environ, {'DISPLAY': ':178'}), patch.object(io, 'open', blocked_open):
            worker = threading.Thread(target=producer, name='fixture-publication')
            worker.start()
            try:
                assert opened.wait(5), f'fixture did not reach write: {errors}'
                # Read without io.open instrumentation while producer is held.
                visible = marker.exists()
                observed = marker.read_bytes() if visible else None
                print(f'blocked: target_exists={visible}; target_bytes={observed!r}', flush=True)
            finally:
                resume.set()
                worker.join(5)
            assert not worker.is_alive(), 'producer thread was not reaped'
        if fail_write:
            assert len(errors) == 1 and isinstance(errors[0], OSError), errors
            assert not marker.exists(), 'failed publication exposed target'
            assert list(Path(directory).iterdir()) == [], 'failed publication left owned temporary'
            print('PASS: failed write left no marker or temporary')
            return
        assert not errors, errors
        expected = ':178; mapped window 42; dimensions 1280x720; green pixel verified'
        assert marker.read_text() == expected
        assert list(Path(directory).iterdir()) == [marker], 'owned temporary leaked'
        if expect_atomic:
            assert not visible, f'publication exposed incomplete marker: {observed!r}'
            print('PASS: target absent until full payload was published; temporary cleaned')
        else:
            assert visible and observed == b'', 'original empty-marker race was not reproduced'
            print('PASS: original empty-marker publication race reproduced deterministically')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('fixture', type=Path)
    parser.add_argument('--expect-atomic', action='store_true')
    parser.add_argument('--fail-write', action='store_true')
    args = parser.parse_args()
    probe(args.fixture, args.expect_atomic, args.fail_write)
