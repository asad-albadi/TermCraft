"""Strict, transactional portable archive extraction on Python 3.11 and later.

Release TARs contain only directories and regular files. Never delegate pathname
or member-type decisions to version-dependent extraction defaults. Only the
Apple Silicon Chromium app can contain relative framework links in a ZIP.
"""
import os
from pathlib import Path, PurePosixPath
import shutil
import stat
import tarfile
import tempfile
import zipfile


MACOS_CHROMIUM = PurePosixPath(
    "TermCraft.app/Contents/Resources/browser/macos-aarch64/Chromium.app"
)


def _zip_path(name):
    path = PurePosixPath(name)
    if (not name or path.as_posix() != name or path.is_absolute() or path == PurePosixPath(".")
            or ".." in path.parts or any(ch in name for ch in ("\\", ":", "\0"))):
        raise ValueError(f"Invalid ZIP path: {name}")
    return path


def extract_zip(archive, directory):
    """Extract regular leaves before making the confined Chromium framework links."""
    destination = Path(os.path.abspath(directory))
    _directory_path(destination)
    if destination.exists() and any(destination.iterdir()):
        raise ValueError("Extraction directory must be empty.")
    with zipfile.ZipFile(archive) as bundle:
        files = {}
        links = {}
        directories = set()
        seen = set()
        for member in bundle.infolist():
            name = member.orig_filename
            path = _zip_path(name.removesuffix("/") if member.is_dir() else name)
            if path in seen:
                raise ValueError(f"Duplicate ZIP member: {name}")
            seen.add(path)
            mode = member.external_attr >> 16
            if member.is_dir():
                if stat.S_IFMT(mode) not in (0, stat.S_IFDIR):
                    raise ValueError(f"Invalid ZIP directory: {name}")
                directories.add(path)
            elif stat.S_ISREG(mode):
                files[path] = member
            elif (stat.S_ISLNK(mode) and path != MACOS_CHROMIUM
                    and path.is_relative_to(MACOS_CHROMIUM) and member.file_size <= 1024):
                links[path] = _zip_path(bundle.read(member).decode("utf-8"))
            else:
                raise ValueError(f"Unexpected ZIP member type: {name}")
        leaves = files.keys() | links.keys()
        for path in seen:
            if any(parent in leaves for parent in path.parents):
                raise ValueError(f"ZIP member has a file or link ancestor: {path}")

        with tempfile.TemporaryDirectory(prefix=".portable-extract-", dir=destination.parent) as temporary:
            staged = Path(temporary) / "payload"
            staged.mkdir(mode=0o755)
            for path in sorted(directories):
                (staged / path).mkdir(parents=True, exist_ok=True)
            for path, member in files.items():
                target = staged / path
                target.parent.mkdir(parents=True, exist_ok=True)
                with bundle.open(member) as reader, target.open("xb") as writer:
                    shutil.copyfileobj(reader, writer)
                target.chmod(((member.external_attr >> 16) & 0o755) | 0o600)
            for path, value in links.items():
                target = staged / path
                target.parent.mkdir(parents=True, exist_ok=True)
                target.symlink_to(value)
            for path in links:
                try:
                    resolved = (staged / path).resolve(strict=True)
                except (OSError, RuntimeError) as error:
                    raise ValueError(f"Invalid Chromium bundle link: {path}") from error
                if not resolved.is_relative_to((staged / MACOS_CHROMIUM).resolve(strict=True)):
                    raise ValueError(f"Chromium bundle link escapes its app: {path}")
            _directory_path(destination)
            os.replace(staged, destination)


def _directory_path(path):
    """Reject symlinks in every existing component, including the destination."""
    for component in (*reversed(path.parents), path):
        try:
            mode = component.lstat().st_mode
        except FileNotFoundError:
            continue
        if not stat.S_ISDIR(mode):
            raise ValueError(f"Extraction path is not a real directory: {component}")


def extract_tar(archive, directory):
    destination = Path(os.path.abspath(directory))
    _directory_path(destination)
    if destination.exists() and any(destination.iterdir()):
        raise ValueError("Extraction directory must be empty.")
    # The caller supplies an existing, private parent directory. A staging tree
    # keeps validation/read failures from leaving a partially extracted package.
    with tarfile.open(archive) as bundle:
        members = bundle.getmembers()
        names = {}
        for member in members:
            if member.type not in (tarfile.REGTYPE, tarfile.AREGTYPE, tarfile.DIRTYPE) or member.sparse is not None:
                raise ValueError(f"Unexpected archive member type: {member.name}")
            name = member.name
            canonical = name.rstrip('/') if member.isdir() else name
            parts = PurePosixPath(canonical)
            if (not canonical or parts.is_absolute() or parts.as_posix() != canonical
                    or '..' in parts.parts or canonical == '.'
                    or '\\' in name or ':' in name or '\x00' in name):
                raise ValueError(f"Invalid archive member: {name}")
            if canonical in names:
                raise ValueError(f"Duplicate archive member: {name}")
            names[canonical] = member
        for name in names:
            for parent in PurePosixPath(name).parents:
                if parent.as_posix() in names and not names[parent.as_posix()].isdir():
                    raise ValueError(f"Archive file used as parent directory: {parent}")
        with tempfile.TemporaryDirectory(prefix='.portable-extract-', dir=destination.parent) as temporary:
            staged = Path(temporary) / 'payload'
            staged.mkdir(mode=0o755)
            for name, member in names.items():
                target = staged.joinpath(*PurePosixPath(name).parts)
                # All names are canonical and every parent is created here;
                # no symlinks, hard links, devices, or pre-existing files enter.
                target.parent.mkdir(mode=0o755, parents=True, exist_ok=True)
                if member.isdir():
                    target.mkdir(mode=0o755, exist_ok=True)
                else:
                    source = bundle.extractfile(member)
                    if source is None:
                        raise ValueError(f"Missing archive file data: {name}")
                    with source, target.open('xb') as output:
                        shutil.copyfileobj(source, output)
                    # Match data-filter file permissions: discard special and
                    # group/other write bits, keep owner read/write, and allow
                    # group/other execution only when owner execution is set.
                    mode = member.mode & 0o755
                    if not mode & 0o100:
                        mode &= ~0o111
                    target.chmod(mode | 0o600)
            for target in staged.rglob('*'):
                if target.is_dir():
                    target.chmod(0o755)
            staged.chmod(0o755)
            _directory_path(destination)
            os.replace(staged, destination)
