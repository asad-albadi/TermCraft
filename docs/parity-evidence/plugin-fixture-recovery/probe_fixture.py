"""Extract and exercise the actual C fixture template from the supplied Rust file."""
import json
from pathlib import Path
import re
import selectors
import subprocess
import sys
import tempfile

text = Path(sys.argv[1]).read_text()
encoded = re.search(r'fs::write\(&source, format!\(("(?:[^"\\]|\\.)*")\)\)', text).group(1)
source = json.loads(encoded).replace('{{', '{').replace('}}', '}').replace('{value}', 'true')
with tempfile.TemporaryDirectory() as temp:
    c = Path(temp) / 'fixture.c'
    exe = Path(temp) / 'fixture'
    c.write_text(source)
    subprocess.run(['cc', '-Wall', '-Wextra', '-Werror', '-o', exe, c], check=True)
    p = subprocess.Popen([exe], stdin=subprocess.PIPE, stdout=subprocess.PIPE)
    try:
        selector = selectors.DefaultSelector()
        selector.register(p.stdout, selectors.EVENT_READ)
        ready = selector.select(0.25)
        selector.close()
        if ready:
            # Reading EOF and wait are completion barriers, not a scheduling delay.
            unsolicited = p.stdout.read()
            status = p.wait(timeout=2)
            try:
                p.stdin.write(b'{}\n')
                p.stdin.flush()
            except BrokenPipeError:
                print(f'RED: fixture exited before input (status={status}, output={unsolicited!r}); write raised BrokenPipeError', flush=True)
            else:
                print(f'RED: fixture responded before input (status={status}, output={unsolicited!r})', flush=True)
            sys.exit(1)
        assert p.poll() is None, 'fixture exited before request'
        p.stdin.write(b'{}\n')
        p.stdin.flush()
        response = p.stdout.readline()
        assert response == b'{"protocol":1,"ok":true}\n', response
        assert p.wait(timeout=2) == 0
        print('GREEN: no response/exit before request; request write succeeds; expected original response received; exit=0')
    finally:
        if p.poll() is None:
            p.kill()
        p.wait()
        # close can report the same already-observed failed buffered write.
        try:
            p.stdin.close()
        except BrokenPipeError:
            pass
        p.stdout.close()
