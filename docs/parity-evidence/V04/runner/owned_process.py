"""Linux-only bounded ownership scopes for this sequential fixture runner.

Only descendants newly created by the current process during a scope are owned.
Existing children are excluded. Signal delivery uses pidfds, which pin the exact
process generation even if a PID is subsequently recycled. Callers must not
start concurrent unrelated child work inside a scope.
"""
import ctypes
import os
from pathlib import Path
import signal
import subprocess
import time


def _children(pid):
    try:
        return [int(n) for n in Path(f'/proc/{pid}/task/{pid}/children').read_text().split()]
    except FileNotFoundError:
        return []


def _identity(pid):
    try:
        # comm can contain spaces or closing parentheses; fields after final ')'.
        fields = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()
        return int(fields[19])  # Linux starttime (field 22)
    except FileNotFoundError:
        return None


class OwnedChildren:
    def __init__(self):
        if ctypes.CDLL(None, use_errno=True).prctl(36, 1, 0, 0, 0) != 0:
            raise OSError(ctypes.get_errno(), 'could not become child subreaper')
        self.parent = os.getpid()
        self.existing = {pid: _identity(pid) for pid in _children(self.parent)}

    def _direct(self):
        return [pid for pid in _children(self.parent)
                if self.existing.get(pid) != _identity(pid)]

    def cleanup(self, timeout=3):
        deadline = time.monotonic() + timeout
        kill_at = time.monotonic() + min(.2, timeout / 4)
        while True:
            direct = self._direct()
            if not direct:
                return
            # Walk only descendants of our new direct children. A detached session
            # remains in this ancestry or is adopted by this scope's subreaper.
            pending = list(direct)
            seen = set()
            while pending:
                pid = pending.pop()
                if pid in seen:
                    continue
                seen.add(pid)
                generation = _identity(pid)
                if generation is None:
                    continue
                try:
                    fd = os.pidfd_open(pid)
                except ProcessLookupError:
                    continue
                try:
                    if _identity(pid) != generation:
                        continue
                    pending.extend(_children(pid))
                    try:
                        signal.pidfd_send_signal(fd, signal.SIGKILL if time.monotonic() >= kill_at else signal.SIGTERM)
                    except ProcessLookupError:
                        pass
                finally:
                    os.close(fd)
            for pid in direct:
                try:
                    os.waitpid(pid, os.WNOHANG)
                except ChildProcessError:
                    pass
            if time.monotonic() >= deadline:
                # The deadline branch must not claim cleanup. Return failure with
                # pinned ownership still explicit, never signal an unrelated PID.
                if self._direct():
                    raise RuntimeError(f'owned descendants exceeded cleanup bound: {self._direct()}')
                return
            time.sleep(.01)


def run_owned(args, *, timeout, input=None, **kwargs):
    """Reap the entire owned tree on success, startup error, timeout or interrupt."""
    scope = OwnedChildren()
    process = None
    try:
        if input is not None:
            kwargs["stdin"] = subprocess.PIPE
        process = subprocess.Popen(args, start_new_session=True, **kwargs)
        stdout, stderr = process.communicate(input=input, timeout=timeout)
        return subprocess.CompletedProcess(args, process.returncode, stdout, stderr)
    finally:
        try:
            scope.cleanup()
        finally:
            if process is not None:
                for stream in (process.stdin, process.stdout, process.stderr):
                    if stream is not None:
                        stream.close()


def install_interrupt_cleanup():
    # Convert ordinary termination into stack unwinding through ownership scopes.
    def terminate(signum, frame):
        raise SystemExit(128 + signum)
    signal.signal(signal.SIGTERM, terminate)
