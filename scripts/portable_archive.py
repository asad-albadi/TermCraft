"""Strict, transactional portable TAR extraction on Python 3.11 and later.

Release TARs contain only directories and regular files. Never delegate pathname
or member-type decisions to version-dependent tarfile extraction defaults.
"""
import os
from pathlib import Path, PurePosixPath
import shutil
import stat
import tarfile
import tempfile


def _directory_path(path):
    """Reject symlinks in every existing component, including the destination."""
    for component in (*reversed(path.parents), path):
        try:
            metadata = component.lstat()
        except FileNotFoundError:
            continue
        if (not stat.S_ISDIR(metadata.st_mode)
                or getattr(metadata, "st_file_attributes", 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT):
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
            if os.name == "nt" and destination.exists():
                # rmdir refuses a nonempty destination even if contents
                # arrived after preflight. Never move it into recursive cleanup.
                destination.rmdir()
                try:
                    staged.rename(destination)
                except OSError:
                    destination.mkdir(exist_ok=True)
                    raise
            else:
                os.replace(staged, destination)
