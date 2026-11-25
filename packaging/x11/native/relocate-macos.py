#!/usr/bin/env python3
"""Stage and sign the recursive Mach-O dependency closure of a private XQuartz image."""

import argparse
from collections import deque
import os
from pathlib import Path
import re
import shutil
import subprocess


SYSTEM = ("/System/Library/", "/usr/lib/")


def output(*args):
    return subprocess.check_output(args, text=True)


def load_names(image, flag):
    lines = output("otool", "-arch", "all", flag, image).splitlines()
    if flag == "-D":
        return {line.strip() for line in lines if line and not line.endswith(":")}
    return {
        match[1]
        for line in lines
        if (match := re.match(r"\s+(.+) \(compatibility version", line))
    }


def rpaths(image):
    return set(re.findall(
        r"cmd LC_RPATH\s+cmdsize \d+\s+path (.+) \(offset \d+\)",
        output("otool", "-arch", "all", "-l", image),
    ))


def dependency_path(name, image, search):
    if name.startswith("/"):
        candidates = [Path(name)]
    elif name.startswith("@rpath/"):
        candidates = [path / name.removeprefix("@rpath/") for path in search]
    elif name.startswith("@loader_path/"):
        candidates = [image.parent / name.removeprefix("@loader_path/")]
    else:
        raise ValueError(f"Unsupported Mach-O install name in {image}: {name}")
    matches = {path.resolve() for path in candidates if path.is_file()}
    if len(matches) != 1:
        raise ValueError(f"Missing or ambiguous Mach-O dependency in {image}: {name}")
    path = matches.pop()
    if path.is_relative_to("/opt/X11") or name.startswith("/opt/X11/"):
        raise ValueError(f"Dependency resolves into installed XQuartz: {name}")
    return path


def relocate(root, arch):
    executable = root / "XQuartz.app/Contents/MacOS/X11.bin"
    compiler = root / "opt/X11/bin/xkbcomp"
    libraries = root / "opt/X11/lib"
    queue = deque([(executable, executable), (compiler, compiler)])
    staged = {}
    required = {"arm64", "x86_64"} if arch == "universal" else {arch}
    while queue:
        original, image = queue.popleft()
        output("lipo", image, "-verify_arch", *sorted(required))
        image.chmod(0o755)
        ids = load_names(original, "-D")
        paths = rpaths(original)
        search = [
            Path(path.replace("@loader_path", str(original.parent)))
            for path in paths
        ]
        changes = []
        for name in sorted(load_names(original, "-L") - ids):
            if name.startswith(SYSTEM):
                continue
            source = dependency_path(name, original, search)
            target = libraries / Path(name).name
            if target in staged and staged[target] != source:
                raise ValueError(f"Conflicting Mach-O dependencies for {target.name}")
            if target not in staged:
                staged[target] = source
                shutil.copyfile(source, target)
                queue.append((source, target))
            relative = os.path.relpath(target, image.parent)
            changes.extend(("-change", name, f"@loader_path/{relative}"))
        if ids:
            changes.extend(("-id", f"@loader_path/{image.name}"))
        for path in sorted(paths):
            changes.extend(("-delete_rpath", path))
        if changes:
            output("install_name_tool", *changes, image)
        for name in load_names(image, "-L") - load_names(image, "-D"):
            if not name.startswith(SYSTEM):
                resolved = dependency_path(name, image, [])
                if not resolved.is_relative_to(libraries):
                    raise ValueError(f"Relocated dependency escapes private lib directory: {name}")
        output("codesign", "--force", "--sign", "-", image)
        output("codesign", "--verify", "--strict", image)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("arch", choices=["arm64", "x86_64", "universal"])
    args = parser.parse_args()
    relocate(args.root.resolve(), args.arch)
