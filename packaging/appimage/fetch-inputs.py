"""Fetch only the checksummed build materials declared in this checkout."""
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
TOOL = ("ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0", "appimagetool",
        "https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage")


def fetch(directory, row):
    sha, name, url = row
    target = directory / name

    def verify(path):
        with path.open("rb") as stream:
            if hashlib.file_digest(stream, "sha256").hexdigest() != sha:
                raise ValueError("Build input checksum differs: " + name)

    if target.exists():
        verify(target)
        return
    with tempfile.NamedTemporaryFile(dir=directory) as temporary:
        # Canonical mirrors retain many of these identical source files.
        locations = [url]
        if url.startswith("https://snapshot.ubuntu.com/") and "/pool/" in url:
            locations.insert(0, "https://archive.ubuntu.com/ubuntu/pool/" + url.split("/pool/")[1])
        for location in locations:
            result = subprocess.run(["curl", "--proto", "=https", "--proto-redir", "=https", "-fsSL",
                                     "--connect-timeout", "20", "--max-time", "900", "--retry", "3",
                                     "-o", temporary.name, location])
            if result.returncode == 0:
                break
        else:
            raise ValueError("Unable to fetch " + name)
        verify(Path(temporary.name))
        os.link(temporary.name, target)


def main():
    directory = Path(sys.argv[1])
    directory.mkdir(parents=True, exist_ok=True)
    lock = json.loads((HERE / "ubuntu-lock.json").read_text())
    items = [line.split() for line in (HERE / "runtime-inputs.tsv").read_text().splitlines()] + [TOOL]
    files = {p["path"]: p["sha256"] for p in lock["packages"].values()}
    for source in lock["sources"].values():
        files.update(source["files"])
    for path, sha in files.items():
        items.append((sha, Path(path).name, lock["snapshot"] + "/" + path))
    if len({item[1] for item in items}) != len(items):
        raise ValueError("Build input names collide")
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        list(pool.map(lambda row: fetch(directory, row), items))


if __name__ == "__main__":
    main()
