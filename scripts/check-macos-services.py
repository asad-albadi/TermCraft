#!/usr/bin/env python3
"""Exercise the registered folder service in an isolated macOS login."""
import argparse
import importlib.util
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time


CALLER = r"""
#import <AppKit/AppKit.h>
int main(int argc, const char **argv) {
    @autoreleasepool {
        if (argc != 4) return 2;
        [NSApplication sharedApplication];
        NSPasteboard *board = [NSPasteboard pasteboardWithUniqueName];
        NSString *path = @(argv[3]);
        BOOL wrote;
        if (strcmp(argv[1], "legacy") == 0) {
            [board declareTypes:@[@"NSFilenamesPboardType"] owner:nil];
            wrote = [board setPropertyList:@[path] forType:@"NSFilenamesPboardType"];
        } else {
            wrote = [board writeObjects:@[[NSURL fileURLWithPath:path isDirectory:YES]]];
        }
        BOOL dispatched = wrote && NSPerformService(@(argv[2]), board);
        [board releaseGlobally];
        return dispatched ? 0 : 1;
    }
}
"""


def wait_for_terminal(folder, launcher, timeout=25):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline and launcher.poll() is None:
        result = subprocess.run(
            ["/usr/sbin/lsof", "-n", "-a", "+d", str(folder), "-d", "cwd", "-Fpcn"],
            capture_output=True, text=True, timeout=5,
        )
        if "n" + str(folder) in result.stdout.splitlines():
            return
        time.sleep(0.25)
    raise RuntimeError(f"No terminal opened at {folder}")


def stop_launcher(launcher):
    if launcher.poll() is None:
        os.killpg(launcher.pid, signal.SIGTERM)
        try:
            launcher.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(launcher.pid, signal.SIGKILL)
            launcher.wait(timeout=5)


def check_services(bundle, destination, fixture):
    source = fixture / "caller.m"
    caller = fixture / "caller"
    source.write_text(CALLER, encoding="utf-8")
    subprocess.run(
        ["xcrun", "clang", "-fobjc-arc", "-framework", "AppKit", str(source), "-o", str(caller)],
        check=True,
    )
    shutil.copytree(bundle, destination, symlinks=True, dirs_exist_ok=True)
    integration_path = destination / "Contents/Resources/desktop-integration.py"
    spec = importlib.util.spec_from_file_location("desktop_integration", integration_path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    integration = module.DesktopIntegration(destination, fixture / "registration.json")
    integration.install_user()
    try:
        if not integration.status():
            raise RuntimeError("LaunchServices did not register the copied bundle")
        workspace = fixture / "workspace"
        launcher = destination / "Contents/MacOS/termcraft-desktop-launcher"
        process = subprocess.Popen(
            [str(launcher), "--data-dir", str(workspace)], start_new_session=True,
        )
        try:
            deadline = time.monotonic() + 20
            while not workspace.is_dir() and process.poll() is None:
                if time.monotonic() >= deadline:
                    raise RuntimeError("The test workspace was not created")
                time.sleep(0.25)
            if process.poll() is not None:
                raise RuntimeError("The test launcher exited before Services dispatch")
            for pasteboard in ("modern", "legacy"):
                folder = fixture / f"{pasteboard} folder ' % sample"
                folder.mkdir()
                subprocess.run(
                    [str(caller), pasteboard, "TermCraft/Open terminal here", str(folder)],
                    check=True, timeout=35,
                )
                wait_for_terminal(folder, process)
                print(f"{pasteboard}: Services opened a terminal at the literal folder", flush=True)
        finally:
            stop_launcher(process)
    finally:
        integration.remove_user()
    if str(destination.resolve()) in integration._registered():
        raise RuntimeError("Fixture remained registered")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("bundle", type=Path, help="Extracted TermCraft.app from the native archive")
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("A macOS desktop login is required")
    apps = Path.home() / "Applications"
    apps.mkdir(exist_ok=True)
    destination = apps / "TermCraft.app"
    # Create exclusively. The cleanup may remove only this invocation's copy.
    destination.mkdir()
    try:
        with tempfile.TemporaryDirectory(prefix="tc-services-", dir="/private/tmp") as temporary:
            check_services(args.bundle.resolve(strict=True), destination, Path(temporary))
    finally:
        shutil.rmtree(destination)


if __name__ == "__main__":
    main()
