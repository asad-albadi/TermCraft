#!/usr/bin/env python3
"""Inspect an exact public candidate and collect native credits, without approving it."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import stat
import sys
import tempfile
import time
from urllib.parse import urlsplit
from urllib.request import urlopen
import zipfile


def require(condition, message):
    if not condition:
        raise ValueError(message)


def safe_path(name):
    path = PurePosixPath(name)
    require(name and path.as_posix() == name and not path.is_absolute()
            and all(re.fullmatch(r"[A-Za-z0-9_. -]+", part) and part not in (".", "..")
                    and not part.endswith((".", " "))
                    and not re.fullmatch(r"(?:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\..*)?", part, re.I)
                    for part in path.parts), "Unsafe candidate archive path")
    return path


def validate_candidate(value):
    require((value["schema_version"], value["scope"], value["platform"], value["architecture"], value["executable"])
            == (1, "qualification-only", "windows", "x86_64", "chrome.exe")
            and value["approved"] is False, "Candidate must remain unapproved qualification-only Windows x64")
    require(re.fullmatch(r"[0-9a-f]{64}", value["archive_sha256"])
            and re.fullmatch(r"[0-9a-f]{40}", value["source_revision"])
            and type(value["archive_size"]) is int and 0 < value["archive_size"] <= 512 * 1024 * 1024,
            "Invalid pinned candidate identity")
    for key in ("archive_url", "source_url"):
        url = urlsplit(value[key])
        require(url.scheme == "https" and url.hostname == "github.com" and not url.username
                and not url.password and url.port in (None, 443), "Expected a public HTTPS GitHub source")
    require(len(safe_path(value["archive_name"]).parts) == 1 and value["archive_name"].endswith(".zip")
            and len(safe_path(value["archive_root"]).parts) == 1, "Invalid candidate archive filename/root")


def digest_file(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def inspect_archive(archive, candidate, *, max_bytes=2 * 1024 * 1024 * 1024, destination=None):
    validate_candidate(candidate)
    require(archive.is_file() and not archive.is_symlink(), "Expected a regular candidate archive")
    with archive.open("rb") as stream:
        require(os.fstat(stream.fileno()).st_size == candidate["archive_size"]
                and hashlib.file_digest(stream, "sha256").hexdigest() == candidate["archive_sha256"],
                "Pinned candidate archive checksum/size mismatch")
        stream.seek(0)
        with zipfile.ZipFile(stream) as package:
            members = package.infolist()
            require(0 < len(members) <= 10000 and sum(m.file_size for m in members) <= max_bytes,
                    "Candidate archive exceeds inventory bound")
            paths, aliases, leaves = {}, set(), set()
            for member in members:
                name = member.orig_filename.removesuffix("/") if member.is_dir() else member.orig_filename
                path = safe_path(name)
                require(path.parts[0] == candidate["archive_root"] and len(path.parts) > 1
                        or (member.is_dir() and path.as_posix() == candidate["archive_root"]),
                        "Unexpected candidate archive root")
                alias = name.casefold()
                mode = stat.S_IFMT(member.external_attr >> 16)
                require(alias not in aliases and not member.flag_bits & 1
                        and mode in ((0, stat.S_IFDIR) if member.is_dir() else (0, stat.S_IFREG)),
                        "Ambiguous or unsupported candidate archive entry")
                aliases.add(alias)
                paths[path] = member
                if not member.is_dir():
                    leaves.add(path)
            leaf_aliases = {path.as_posix().casefold() for path in leaves}
            require(all(not any(parent.as_posix().casefold() in leaf_aliases for parent in path.parents) for path in paths),
                    "Candidate file used as an archive parent")
            files = []
            for path, member in sorted(paths.items()):
                if member.is_dir():
                    continue
                relative = path.relative_to(candidate["archive_root"]).as_posix()
                digest, length, header = hashlib.sha256(), 0, bytearray()
                target = destination / relative if destination is not None else None
                if target:
                    target.parent.mkdir(parents=True, exist_ok=True)
                output = target.open("xb") if target else None
                try:
                    with package.open(member) as reader:
                        while chunk := reader.read(65536):
                            length += len(chunk)
                            require(length <= member.file_size, "Candidate member exceeds declared bound")
                            digest.update(chunk)
                            if len(header) < 65536:
                                header.extend(chunk[:65536 - len(header)])
                            if output:
                                output.write(chunk)
                finally:
                    if output:
                        output.close()
                require(length == member.file_size, "Truncated candidate member")
                entry = {"path": relative, "size": length, "sha256": digest.hexdigest()}
                if path.suffix.lower() in (".exe", ".dll"):
                    require(len(header) >= 64 and header[:2] == b"MZ", "Invalid candidate PE image")
                    offset = int.from_bytes(header[60:64], "little")
                    require(offset + 6 <= len(header) and header[offset:offset + 6] == b"PE\0\0\x64\x86",
                            "Candidate PE image is not x64")
                    entry["machine"] = "x86_64"
                files.append(entry)
            require(any(f["path"] == candidate["executable"] and f.get("machine") == "x86_64" for f in files),
                    "Candidate executable is missing")
    return {"scope": "qualification-only", "approved": False,
            "archive_sha256": candidate["archive_sha256"], "files": files}


def credits_summary(data):
    require(16384 <= len(data) <= 64 * 1024 * 1024, "Credits output is incomplete or exceeds bound")
    try:
        text = data.decode("utf-8").lower()
    except UnicodeDecodeError as error:
        raise ValueError("Credits output is not UTF-8 HTML") from error
    require("<html" in text and "</html>" in text and text.count("copyright") >= 10
            and all(marker not in text for marker in ("generate_about_credits=true", "sample text")),
            "Credits output is missing notices or contains a known placeholder")
    return {"sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data),
            "copyright_occurrences": text.count("copyright"), "redistribution_approved": False,
            "scope": "non-placeholder native capture; completeness and redistribution require review"}


def download(candidate, destination):
    deadline = time.monotonic() + 300
    with urlopen(candidate["archive_url"], timeout=30) as response, destination.open("xb") as output:
        require(urlsplit(response.url).scheme == "https", "Candidate download downgraded transport")
        length = 0
        while chunk := response.read(65536):
            length += len(chunk)
            require(length <= candidate["archive_size"] and time.monotonic() < deadline,
                    "Candidate download exceeds pinned size/time bound")
            output.write(chunk)
    require(length == candidate["archive_size"], "Truncated candidate download")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--archive", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--inspect-only", action="store_true")
    args = parser.parse_args()
    candidate = json.loads(args.candidate.read_text())
    validate_candidate(candidate)
    require(args.inspect_only or sys.platform == "win32", "Native credits capture requires Windows")
    args.output.mkdir(parents=True, exist_ok=False)
    shutil.copyfile(args.candidate, args.output / "candidate.json")
    report = {"schema_version": 1, "scope": "qualification-only", "approved": False,
              "candidate": candidate, "native_capture": False, "redistribution_approved": False}
    try:
        with tempfile.TemporaryDirectory(prefix=".browser-intake-", dir=args.output) as temporary:
            work = Path(temporary).resolve()
            archive = args.archive.resolve() if args.archive else work / candidate["archive_name"]
            if not args.archive:
                download(candidate, archive)
            runtime = work / "runtime"
            inventory = inspect_archive(archive, candidate, destination=runtime if not args.inspect_only else None)
            (args.output / "archive-inventory.json").write_text(json.dumps(inventory, indent=2) + "\n")
            report["inventory_sha256"] = digest_file(args.output / "archive-inventory.json")
            if not args.inspect_only:
                spec = importlib.util.spec_from_file_location("windows_browser_intake", Path(__file__).with_name("windows-browser-intake.py"))
                runner = importlib.util.module_from_spec(spec)
                spec.loader.exec_module(runner)
                stdout, stderr = (args.output / "THIRD-PARTY-NOTICES.html").resolve(), (args.output / "browser.log").resolve()
                report["process"] = runner.capture(runtime / candidate["executable"], work / "profile", stdout, stderr)
                require(report["process"]["exit_code"] == 0 and not report["process"]["timed_out"],
                        "Native credits capture did not complete successfully")
                require(stdout.stat().st_size <= 64 * 1024 * 1024, "Credits output exceeds bound")
                with stdout.open("rb") as captured:
                    report["credits"] = credits_summary(captured.read(64 * 1024 * 1024 + 1))
                report["native_capture"] = True
        report["temporary_runtime_removed"] = True
    except Exception as error:
        report["error"] = str(error)
        raise
    finally:
        (args.output / "intake.json").write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
