"""Start two pinned xdm processes in the Rust test's private namespace."""

import os
import pathlib
import shlex
import signal
import subprocess
import sys
from contextlib import ExitStack

assert os.environ.get("TERMCRAFT_TEST_XSERVER_ISOLATED") == "1"
fixture = pathlib.Path(os.environ["TERMCRAFT_TEST_XDMCP_FIXTURE"])


def manager(stack, path, port, address, indirect=False):
    path.mkdir()
    (path / "auth").mkdir()
    (path / "servers").touch()
    chooser_rule = "* CHOOSER 127.0.0.2\n" if indirect else ""
    (path / "access").write_text(f"LISTEN {address}\n*\n{chooser_rule}")
    setup = path / "setup"
    # The xdm setup script reports which distinct display actually authenticated.
    script = shlex.quote(str(fixture / "xdmcp_window.py"))
    marker = shlex.quote(str(path / "managed-")) + '"${DISPLAY##*:}"'
    setup.write_text(f"#!/bin/sh\nexec /usr/bin/python {script} {marker}\n")
    setup.chmod(0o700)
    # xdm splits the chooser command string at spaces. The actual chooser
    # remains the exact archive binary, reached via a private path without spaces.
    (path / "chooser").symlink_to(fixture / "chooser")
    params = {
        ".servers": path / "servers",
        ".accessFile": path / "access",
        ".authDir": path / "auth",
        ".pidFile": path / "pid",
        ".errorLogFile": path / "error",
        ".requestPort": port,
        ".daemonMode": "false",
        ".exportList": "LD_LIBRARY_PATH XAPPLRESDIR HOME",
        "*authorize": "true",
        "*authName": "MIT-MAGIC-COOKIE-1",
        "*setup": setup,
        "*openDelay": 1,
        "*openRepeat": 3,
        "*greeterLib": fixture / "libXdmGreet.so",
        "*chooser": path / "chooser",
        "*resources": "",
        "*startup": "",
        "*reset": "",
    }
    config = path / "config"
    config.write_text("".join(f"DisplayManager{key}: {value}\n" for key, value in params.items()))
    # This private, ephemeral diagnostic may contain cookie bytes. Never print it.
    log = stack.enter_context((path / "log").open("wb"))
    env = {
        **os.environ,
        "LD_LIBRARY_PATH": str(fixture / "lib"),
        "XAPPLRESDIR": str(fixture / "app-defaults"),
    }
    child = subprocess.Popen(
        [fixture / "xdm", "-nodaemon", "-debug", "1", "-config", config],
        stdout=log,
        stderr=log,
        start_new_session=True,
        env=env,
    )
    stack.callback(stop, child)
    return child


def stop(child):
    if child.poll() is not None:
        return
    os.killpg(child.pid, signal.SIGTERM)
    try:
        child.wait(timeout=2)
    except subprocess.TimeoutExpired:
        os.killpg(child.pid, signal.SIGKILL)
        child.wait()


with ExitStack() as stack:
    root = pathlib.Path(sys.argv[1])
    initial = manager(stack, root / "initial", 1177, "127.0.0.1", True)
    selected = manager(stack, root / "selected", 177, "127.0.0.2")
    # The owning Rust test closes stdin and joins us before dropping this directory.
    assert sys.stdin.buffer.read(1) == b""
    assert initial.poll() is None
    assert selected.poll() is None
